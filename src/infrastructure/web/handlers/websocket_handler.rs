use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::FromRequest;
use axum::http::header::AUTHORIZATION;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tracing::{error, info, warn};

use crate::application::services::DashboardService;
use crate::infrastructure::web::viewhelpers;
use crate::infrastructure::web::AppState;

/// Parse a query parameter value from a query string.
fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    query?
        .split('&')
        .find_map(|p| p.strip_prefix(&format!("{key}=")))
        .and_then(|v| {
            v.splitn(2, '=')
                .last()
                .map(|s| s.to_string())
        })
}

/// Parse a Bearer token from an Authorization header.
fn bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Validate the WebSocket auth token. Returns true if auth passes or is not configured.
fn check_ws_auth(headers: &axum::http::HeaderMap, uri: &axum::http::Uri, expected_token: &str) -> bool {
    let auth = bearer_token(headers)
        .or(query_param(uri.query(), "token"))
        .unwrap_or_default();
    auth == expected_token
}

pub async fn ws_solar(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    // Check auth before upgrading
    let expected_token = state
        .dashboard_service
        .config()
        .ws_auth_token
        .clone();
    if let Some(ref token) = expected_token {
        if !check_ws_auth(request.headers(), request.uri(), token) {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    }

    // Upgrade the connection
    let upgrade = match WebSocketUpgrade::from_request(request, &state).await {
        Ok(u) => u,
        Err(rejection) => return rejection.into_response(),
    };

    upgrade.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    info!("WebSocket client connected to /ws/solar");

    let (socket_sink, socket_stream) = socket.split();
    let mut broadcast_rx = state.dashboard_service.subscribe_to_ws();

    // Single channel for all outgoing messages (broadcast + command responses)
    let (outgoing_tx, outgoing_rx) = tokio::sync::mpsc::channel::<Message>(64);

    // Task 1: broadcast → outgoing channel
    let broadcast_tx = outgoing_tx.clone();
    let broadcast_handle = tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(payload) => {
                    let msg = Message::Text(payload.to_string().into());
                    if broadcast_tx.send(msg).await.is_err() {
                        break; // receiver dropped
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("WebSocket lagged behind, skipped {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // Task 2: handle client commands → outgoing channel
    let cmd_tx = outgoing_tx.clone();
    let service = state.dashboard_service.clone();
    let mut stream = socket_stream;
    while let Some(Ok(msg)) = stream.next().await {
        if let Message::Text(text) = msg {
            let payload = handle_client_command(&text, &service).await;
            let response_msg = Message::Text(payload.to_string().into());
            if cmd_tx.send(response_msg).await.is_err() {
                warn!("Failed to send WS command response: channel closed");
                break;
            }
        }
    }

    // Client disconnected: abort broadcast task to stop the zombie,
    // then drain any remaining messages through the sink.
    broadcast_handle.abort();
    drop(cmd_tx);
    drop(outgoing_tx);

    // Drain remaining messages to the WebSocket sink
    let mut sink = socket_sink;
    let mut rx = outgoing_rx;
    while let Some(msg) = rx.recv().await {
        if sink.send(msg).await.is_err() {
            break;
        }
    }

    info!("WebSocket client disconnected from /ws/solar");
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Command Parsing Tests ──────────────────────────────────────────

    #[test]
    fn test_parse_toggle_command() {
        let json = r#"{"action":"toggle","entity_id":"light.test"}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        match cmd {
            ClientCommand::Toggle { entity_id } => assert_eq!(entity_id, "light.test"),
            _ => panic!("Expected Toggle"),
        }
    }

    #[test]
    fn test_parse_run_script_command() {
        let json = r#"{"action":"run_script","entity_id":"script.away"}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        match cmd {
            ClientCommand::RunScript { entity_id } => assert_eq!(entity_id, "script.away"),
            _ => panic!("Expected RunScript"),
        }
    }

    #[test]
    fn test_parse_save_settings_command() {
        let json = r#"{"action":"save_settings","visible":["light.a","light.b"],"pages":{"light.a":0,"light.b":1}}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        match cmd {
            ClientCommand::SaveSettings { visible, pages } => {
                assert_eq!(visible, vec!["light.a".to_string(), "light.b".to_string()]);
                assert_eq!(pages.len(), 2);
                assert_eq!(pages.get("light.a"), Some(&0));
                assert_eq!(pages.get("light.b"), Some(&1));
            }
            _ => panic!("Expected SaveSettings"),
        }
    }

    #[test]
    fn test_parse_force_refresh_command() {
        let json = r#"{"action":"force_refresh"}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        matches!(cmd, ClientCommand::ForceRefresh {});
    }

    // ── Error Cases ────────────────────────────────────────────────────

    #[test]
    fn test_parse_invalid_action() {
        let json = r#"{"action":"unknown_action","entity_id":"light.test"}"#;
        let result: Result<ClientCommand, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Should reject unknown action");
    }

    #[test]
    fn test_parse_missing_entity_id_toggle() {
        let json = r#"{"action":"toggle"}"#;
        let result: Result<ClientCommand, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Should reject toggle without entity_id");
    }

    #[test]
    fn test_parse_missing_entity_id_run_script() {
        let json = r#"{"action":"run_script"}"#;
        let result: Result<ClientCommand, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Should reject run_script without entity_id"
        );
    }

    #[test]
    fn test_parse_missing_required_fields_save_settings() {
        let json = r#"{"action":"save_settings"}"#;
        let result: Result<ClientCommand, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Should reject save_settings without required fields"
        );
    }

    #[test]
    fn test_parse_empty_string() {
        let result: Result<ClientCommand, _> = serde_json::from_str("");
        assert!(result.is_err(), "Should reject empty string");
    }

    #[test]
    fn test_parse_malformed_json() {
        let result: Result<ClientCommand, _> = serde_json::from_str("{bad");
        assert!(result.is_err(), "Should reject malformed JSON");
    }

    #[test]
    fn test_parse_camelcase_rejected() {
        // camelCase should be rejected due to rename_all = "snake_case"
        let json = r#"{"action":"toggle","entityId":"light.test"}"#;
        let result: Result<ClientCommand, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Should reject camelCase field names");
    }

    #[test]
    fn test_parse_extra_fields_rejected() {
        // deny_unknown_fields should reject extra keys
        let json = r#"{"action":"toggle","entity_id":"light.test","extra_field":"value"}"#;
        let result: Result<ClientCommand, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Should reject extra unknown fields");
    }

    #[test]
    fn test_parse_force_refresh_no_extra_fields() {
        let json = r#"{"action":"force_refresh","extra":"value"}"#;
        let result: Result<ClientCommand, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Should reject force_refresh with extra fields"
        );
    }

    #[test]
    fn test_parse_empty_json_object() {
        let result: Result<ClientCommand, _> = serde_json::from_str("{}");
        // With tag = "action", empty object should fail to deserialize
        assert!(result.is_err(), "Should reject empty JSON object");
    }

    #[test]
    fn test_parse_toggle_with_wrong_type() {
        // entity_id should be a string, not a number
        let json = r#"{"action":"toggle","entity_id":123}"#;
        let result: Result<ClientCommand, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Should reject non-string entity_id");
    }

    #[test]
    fn test_parse_save_settings_empty_vectors_and_maps() {
        let json = r#"{"action":"save_settings","visible":[],"pages":{}}"#;
        let cmd: ClientCommand = serde_json::from_str(json).unwrap();
        match cmd {
            ClientCommand::SaveSettings { visible, pages } => {
                assert!(visible.is_empty());
                assert!(pages.is_empty());
            }
            _ => panic!("Expected SaveSettings"),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[serde(tag = "action")]
enum ClientCommand {
    Toggle {
        entity_id: String,
    },
    RunScript {
        entity_id: String,
    },
    SaveSettings {
        visible: Vec<String>,
        pages: std::collections::HashMap<String, usize>,
    },
    ForceRefresh {},
}

async fn handle_client_command(text: &str, service: &DashboardService) -> serde_json::Value {
    let cmd: ClientCommand = match serde_json::from_str(text) {
        Ok(cmd) => cmd,
        Err(e) => {
            warn!("WS invalid command: {}", e);
            return serde_json::json!({
                "type": "error",
                "message": format!("Invalid command: {}", e),
            });
        }
    };

    match cmd {
        ClientCommand::Toggle { entity_id } => {
            info!("WS toggle: {}", entity_id);
            let id = crate::domain::EntityId(entity_id);
            if let Err(e) = service.toggle_entity(&id).await {
                error!("WS toggle failed: {}", e);
                return serde_json::json!({
                    "type": "error",
                    "message": format!("Toggle failed: {}", e),
                });
            }
        }
        ClientCommand::RunScript { entity_id } => {
            info!("WS run_script: {}", entity_id);
            let id = crate::domain::EntityId(entity_id);
            if let Err(e) = service.run_script(&id).await {
                error!("WS run_script failed: {}", e);
                return serde_json::json!({
                    "type": "error",
                    "message": format!("Script failed: {}", e),
                });
            }
        }
        ClientCommand::SaveSettings { visible, pages } => {
            // Validate page numbers are >= 1 (1-based indexing)
            let pages: std::collections::HashMap<String, usize> = pages
                .into_iter()
                .filter(|(_, p)| *p >= 1)
                .collect();
            info!(
                "WS save_settings: visible={}, pages={}",
                visible.len(),
                pages.len()
            );
            let ids: Vec<crate::domain::EntityId> = visible
                .into_iter()
                .map(crate::domain::EntityId)
                .collect();
            if let Err(e) = service.save_visible_entities(ids).await {
                error!("WS save visible failed: {}", e);
                return serde_json::json!({
                    "type": "error",
                    "message": format!("Save visible failed: {}", e),
                });
            }
            if let Err(e) = service.save_entity_pages(pages).await {
                error!("WS save pages failed: {}", e);
                return serde_json::json!({
                    "type": "error",
                    "message": format!("Save pages failed: {}", e),
                });
            }
        }
        ClientCommand::ForceRefresh {} => {
            info!("WS force_refresh");
        }
    }

    // Build and return fresh payload
    build_fresh_payload(service).await
}

async fn build_fresh_payload(service: &DashboardService) -> serde_json::Value {
    let state = match service.get_dashboard().await {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({
                "type": "error",
                "message": format!("Failed to get dashboard: {}", e),
            });
        }
    };

    let cfg = service.config();

    let watts = service.parse_solar_watts(&state);
    let chart_points = service.compute_solar_chart().await;
    let buffer_chart_points = service.compute_buffer_temp_chart().await;
    let pump_status = service.compute_pump_status(&state);
    let charger_state = service.compute_car_state(&state).await;

    let garage_left = viewhelpers::build_garage_door(
        cfg.garage_left_status_entity_id.as_str(),
        "Garage Left",
        &state,
    );
    let garage_right = viewhelpers::build_garage_door(
        cfg.garage_right_status_entity_id.as_str(),
        "Garage Right",
        &state,
    );

    let chart_labels: Vec<String> = chart_points.labels;
    let chart_values: Vec<f64> = chart_points.values;

    let buffer_labels = buffer_chart_points.labels;
    let buffer_top_vals: Vec<String> = buffer_chart_points
        .buffer_top
        .iter()
        .map(|v| format!("{:.1}", v))
        .collect();
    let buffer_bottom_vals: Vec<String> = buffer_chart_points
        .buffer_bottom
        .iter()
        .map(|v| format!("{:.1}", v))
        .collect();
    let solar_flow_vals: Vec<String> = buffer_chart_points
        .solar_flow
        .iter()
        .map(|v| format!("{:.1}", v))
        .collect();
    let solar_return_vals: Vec<String> = buffer_chart_points
        .solar_return
        .iter()
        .map(|v| format!("{:.1}", v))
        .collect();

    serde_json::json!({
        "watts": watts,
        "max_watts": cfg.solar_max_watts,
        "percent": if cfg.solar_max_watts > 0.0 {
            ((watts / cfg.solar_max_watts) * 100.0).round().clamp(0.0, 100.0) as u8
        } else {
            0
        },
        "chart_labels": chart_labels,
        "chart_values": chart_values,
        "charger_amps": charger_state.charger_value,
        "charger_status": charger_state.status,
        "charger_car_state": charger_state.car_state_label,
        "charger_car_state_class": charger_state.car_state_class,
        "charger_car_connected": charger_state.car_connected,
        "charger_charging": charger_state.car_charging,
        "charger_paused": charger_state.paused,
        "garage_left": garage_left,
        "garage_right": garage_right,
        "buffer_temps": {
            "labels": buffer_labels,
            "buffer_top": buffer_top_vals,
            "buffer_bottom": buffer_bottom_vals,
            "solar_flow": solar_flow_vals,
            "solar_return": solar_return_vals,
        },
        "pump_status": {
            "pump_on": pump_status.pump_on,
            "is_correct": pump_status.is_correct,
            "status_label": pump_status.status_label,
            "css_class": pump_status.css_class,
        },
    })
}
