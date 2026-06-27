use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tracing::{info, warn, error};

use crate::application::services::DashboardService;
use crate::infrastructure::web::AppState;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
#[serde(tag = "action")]
enum ClientCommand {
    Toggle { entity_id: String },
    RunScript { entity_id: String },
    SaveSettings {
        visible: Vec<String>,
        pages: std::collections::HashMap<String, usize>,
    },
    ForceRefresh,
}

pub async fn ws_solar(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    info!("WebSocket client connected to /ws/solar");

    let (socket_sink, socket_stream) = socket.split();
    let mut broadcast_rx = state.dashboard_service.subscribe_to_ws();

    // Single channel for all outgoing messages (broadcast + command responses)
    let (outgoing_tx, outgoing_rx) = tokio::sync::mpsc::channel::<Message>(64);

    // Task 1: send all outgoing messages to the WebSocket
    tokio::spawn(async move {
        let mut sink = socket_sink;
        let mut rx = outgoing_rx;
        while let Some(msg) = rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Task 2: broadcast channel → outgoing channel
    let broadcast_tx = outgoing_tx.clone();
    tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(payload) => {
                    let msg = Message::Text(payload.to_string().into());
                    if broadcast_tx.send(msg).await.is_err() {
                        break; // sender dropped
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

    // Task 3: handle client commands → outgoing channel
    let cmd_tx = outgoing_tx.clone();
    let service = state.dashboard_service.clone();
    tokio::spawn(async move {
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
    });

    // Wait for client to disconnect (socket_stream exhausted)
    info!("WebSocket client disconnected from /ws/solar");
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
            info!("WS save_settings: visible={}, pages={}", visible.len(), pages.len());
            let ids: Vec<crate::domain::EntityId> = visible.into_iter().map(|s| crate::domain::EntityId(s)).collect();
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
        ClientCommand::ForceRefresh => {
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

    let make_garage_status = |entity_id: &str, default_name: &str| -> serde_json::Value {
        state.entities.iter()
            .find(|e| e.id.0 == entity_id)
            .map(|e| {
                let is_open = e.is_on;
                let name = e.name.clone();
                let status = if is_open { "Offen" } else { "Geschlossen" };
                let action = if is_open { "Schließen" } else { "Öffnen" };
                let button_class = if is_open {
                    "garage-btn garage-open"
                } else {
                    "garage-btn garage-closed"
                };
                serde_json::json!({
                    "name": name,
                    "status": status,
                    "action": action,
                    "button_class": button_class,
                })
            })
            .unwrap_or_else(|| {
                serde_json::json!({
                    "name": default_name,
                    "status": "Geschlossen",
                    "action": "Öffnen",
                    "button_class": "garage-btn garage-closed",
                })
            })
    };

    let garage_left = make_garage_status(cfg.garage_left_status_entity_id.as_str(), "Garage Left");
    let garage_right = make_garage_status(cfg.garage_right_status_entity_id.as_str(), "Garage Right");

    let chart_labels: Vec<String> = chart_points.labels;
    let chart_values: Vec<f64> = chart_points.values;

    let buffer_labels = buffer_chart_points.labels;
    let buffer_top_vals: Vec<String> = buffer_chart_points.buffer_top.iter().map(|v| format!("{:.1}", v)).collect();
    let buffer_bottom_vals: Vec<String> = buffer_chart_points.buffer_bottom.iter().map(|v| format!("{:.1}", v)).collect();
    let solar_flow_vals: Vec<String> = buffer_chart_points.solar_flow.iter().map(|v| format!("{:.1}", v)).collect();
    let solar_return_vals: Vec<String> = buffer_chart_points.solar_return.iter().map(|v| format!("{:.1}", v)).collect();

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
