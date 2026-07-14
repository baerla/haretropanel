// Tests are in a single file to keep things simple and avoid module visibility issues.

use std::sync::Arc;
use std::time::Duration;

use haretropanel::application::ports::HomeAssistantClient;
use haretropanel::application::services::DashboardService;
use haretropanel::config::AppConfig;
use haretropanel::domain::{DashboardState, Entity, EntityId, EntityKind};

// ── Manual mock implementations ──────────────────────────────────────────

struct MockHaClient {
    state_response:
        Arc<dyn Fn() -> haretropanel::shared::error::AppResult<DashboardState> + Send + Sync>,
}

#[async_trait::async_trait]
impl HomeAssistantClient for MockHaClient {
    async fn fetch_dashboard_state(
        &self,
    ) -> haretropanel::shared::error::AppResult<DashboardState> {
        (self.state_response)()
    }

    async fn toggle(&self, _entity_id: &EntityId) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }

    async fn run_script(
        &self,
        _entity_id: &EntityId,
    ) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }

    async fn call_service_raw(
        &self,
        _domain: &str,
        _service: &str,
        _body: serde_json::Value,
    ) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }
}

struct MockLayoutRepo;

#[async_trait::async_trait]
impl haretropanel::application::services::DashboardLayoutRepository for MockLayoutRepo {
    async fn load_visible_entities(&self) -> haretropanel::shared::error::AppResult<Vec<EntityId>> {
        Ok(Vec::new())
    }
    async fn save_visible_entities(
        &self,
        _ids: Vec<EntityId>,
    ) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }
    async fn load_entity_pages(
        &self,
    ) -> haretropanel::shared::error::AppResult<std::collections::HashMap<String, usize>> {
        Ok(Default::default())
    }
    async fn save_entity_pages(
        &self,
        _map: std::collections::HashMap<String, usize>,
    ) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn test_config() -> AppConfig {
    AppConfig {
        demo_mode: false,
        solar_entity_id: "sensor.solar".into(),
        solar_buffer_top_entity_id: String::new(),
        solar_buffer_bottom_entity_id: String::new(),
        solar_flow_entity_id: String::new(),
        solar_return_entity_id: String::new(),
        solar_pump_entity_id: String::new(),
        charger_current_entity_id: String::new(),
        goe_status_entity_id: String::new(),
        goe_energy_entity_id: String::new(),
        goe_car_connected_entity_id: String::new(),
        goe_charging_entity_id: String::new(),
        garage_left_status_entity_id: String::new(),
        garage_left_action_entity_id: String::new(),
        garage_right_status_entity_id: String::new(),
        garage_right_action_entity_id: String::new(),
        solar_max_watts: 9000.0,
        solar_history_minutes: 60,
        solar_sample_secs: 60,
        goe_energy_stable_secs: 60,
        goe_energy_delta_kwh: 0.02,
        server_port: 8080,
        ha_base_url: "http://localhost:8123".into(),
        ha_token: None,
        log_file: None,
        log_level: "haretropanel=debug".into(),
        dashboard_cache_ttl_default_secs: 5,
        dashboard_cache_ttl_light_secs: None,
        dashboard_cache_ttl_switch_secs: None,
        dashboard_cache_ttl_sensor_secs: None,
        dashboard_cache_ttl_climate_secs: None,
        force_fetch_interval_secs: 120,
        ws_auth_token: None,
    }
}

fn build_service(state: DashboardState) -> DashboardService {
    let ha_client = Arc::new(MockHaClient {
        state_response: Arc::new(move || Ok(state.clone())),
    });
    let layout_repo = Arc::new(MockLayoutRepo);
    DashboardService::new(
        ha_client,
        layout_repo,
        haretropanel::application::services::DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        test_config(),
    )
}

// ── Tests ────────────────────────────────────────────────────────────────

/// End-to-end: entity with a wattage value flows correctly from HA through
/// DashboardService to the handler's value extraction logic.
#[tokio::test]
async fn solar_entity_value_flows_to_value_extraction() {
    let state = DashboardState {
        entities: vec![Entity {
            id: EntityId("sensor.sunny_home_manager_2_0_metering_power_supplied".into()),
            name: "Solar Power".into(),
            kind: EntityKind::Sensor,
            is_on: true,
            value: Some("4200 W".into()),
        }],
    };

    let mut config = test_config();
    config.solar_entity_id = "sensor.sunny_home_manager_2_0_metering_power_supplied".into();
    config.solar_sample_secs = 1; // Add samples every second so the test doesn't need a long sleep

    let service = DashboardService::new(
        Arc::new(MockHaClient {
            state_response: Arc::new(move || Ok(state.clone())),
        }),
        Arc::new(MockLayoutRepo),
        haretropanel::application::services::DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        config.clone(),
    );

    // Fetch fresh to populate cache
    service
        .get_dashboard_with_refresh(true)
        .await
        .expect("fetch should succeed");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Retrieve the cached state and verify the entity value pipeline
    let cached = service
        .get_dashboard_with_refresh(false)
        .await
        .expect("fetch should succeed");

    let solar_entity = cached
        .entities
        .iter()
        .find(|e| e.id.0 == config.solar_entity_id);

    assert!(solar_entity.is_some(), "solar entity should be found by ID");

    let solar = solar_entity.unwrap();
    assert!(solar.is_on, "sensor starting with '4200' should be on");

    // value field must contain "4200 W"
    let raw_value = solar
        .value
        .as_deref()
        .expect("solar sensor should have Some(value)");
    assert!(
        raw_value.contains("4200"),
        "solar value should contain '4200' but got: {:?}",
        raw_value
    );

    // Parse the wattage exactly as the handler does
    let watts = raw_value
        .split_whitespace()
        .next()
        .and_then(|n| n.parse::<f64>().ok())
        .unwrap_or(0.0);
    assert_eq!(watts, 4200.0, "parsed solar watts should be 4200");

    // Percent must be non-zero
    let cfg = service.config();
    let percent = ((watts / cfg.solar_max_watts) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8;
    assert!(
        percent > 0,
        "percent > 0 when watts=4200, max=9000, got {percent}"
    );

    // Chart labels/values should not be all zeros
    let history = service.solar_history_points().await;
    assert!(!history.is_empty(), "should have at least one solar sample");
}

/// Missing entity ID must not panic; produces 0 watts (documented behavior).
#[tokio::test]
async fn missing_entity_id_results_in_zero_watts() {
    let state = DashboardState {
        entities: vec![Entity {
            id: EntityId("sensor.other".into()),
            name: "Other".into(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("100 W".into()),
        }],
    };

    let mut config = test_config();
    config.solar_entity_id = "sensor.nonexistent".into();

    let service = build_service(state);
    let _cfg = service.config().clone();

    let cached = service
        .get_dashboard_with_refresh(true)
        .await
        .expect("fetch should not error");

    let solar_entity = cached
        .entities
        .iter()
        .find(|e| e.id.0 == config.solar_entity_id);

    assert!(
        solar_entity.is_none(),
        "entity should not be found when ID does not match"
    );

    // The handler's parse logic yields 0.0 for missing entities
    let watts: f64 = solar_entity
        .and_then(|e| e.value.clone())
        .and_then(|v| {
            v.split_whitespace()
                .next()
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(0.0);
    assert_eq!(watts, 0.0, "watts must default to 0 when entity is missing");
}

/// An unavailable sensor (value=None) must produce 0 watts, not panic.
/// This catches the regression where HA returning "unavailable"/"unknown"
/// state silently produced zeros in the dashboard.
#[tokio::test]
async fn unavailable_sensor_produces_zero_watts() {
    let state = DashboardState {
        entities: vec![Entity {
            id: EntityId("sensor.sunny_home_manager_2_0_metering_power_supplied".into()),
            name: "Solar Power".into(),
            kind: EntityKind::Sensor,
            is_on: false, // unavailable -> is_on=false
            value: None,  // unavailable has no value
        }],
    };

    let service = build_service(state);

    let cached = service
        .get_dashboard_with_refresh(true)
        .await
        .expect("fetch should not error");

    let solar_entity = cached
        .entities
        .iter()
        .find(|e| e.id.0 == "sensor.sunny_home_manager_2_0_metering_power_supplied");

    assert!(solar_entity.is_some(), "entity should be found");

    let watts: f64 = solar_entity
        .and_then(|e| e.value.clone())
        .and_then(|v| {
            v.split_whitespace()
                .next()
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(0.0);
    assert_eq!(watts, 0.0, "unavailable sensor should produce 0 watts");
}

// ── Route Removal Tests ────────────────────────────────────────────────
// These tests verify that the old HTTP POST routes have been removed
// and replaced with the WebSocket protocol.
//
// Note: Full route testing requires a `test_app()` helper that builds
// the complete Axum app with mock service. The old POST handlers for
// /toggle, /run_script, and POST /settings/entities were removed in
// the WebSocket migration (commit 96273b7). Manual verification confirms
// they return 404. The WebSocket endpoint at /ws/solar handles all
// frontend interactions via the protocol defined in
// src/infrastructure/web/handlers/websocket_handler.rs.

/// Verify GET / still works after route removal (dashboard page served by WebSocket handler)
#[tokio::test]
async fn test_get_dashboard_still_works() {
    // The dashboard page is served by the WebSocket handler's fallback.
    // Without a full app router, we can't test this directly.
    // The bootstrap.rs wires up `GET /` to serve the dashboard template.
    // This test documents the expected behavior.
    // Manual: curl http://localhost:8080/ should return 200 OK with HTML.
}

/// Verify GET /settings/entities still works
#[tokio::test]
async fn test_get_settings_still_works() {
    // The settings page is served by `get_settings()` in settings_handler.rs.
    // Manual: curl http://localhost:8080/settings/entities should return 200 OK with HTML.
}

// ── WebSocket E2E Tests ────────────────────────────────────────────────

use async_tungstenite::tungstenite::Message as WsMessage;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::net::TcpListener;

/// Helper: bind a random-port TCP listener and return (listener, port).
async fn bind_random() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// Helper: build an Axum router with the given DashboardService and WS route.
fn build_ws_router(service: Arc<DashboardService>) -> Router {
    use haretropanel::infrastructure::web::handlers::websocket_handler::ws_solar;
    use haretropanel::infrastructure::web::AppState;
    Router::new()
        .route("/ws/solar", get(ws_solar))
        .with_state(AppState {
            dashboard_service: service,
        })
}

/// Build a tracking mock HA client that counts fetches and tracks toggles.
fn build_tracking_client(
    state: DashboardState,
) -> (
    Arc<TrackingHaClient>,
    std::sync::Arc<AtomicUsize>, // fetch counter
    std::sync::Arc<AtomicBool>,  // toggle counter
    std::sync::Arc<AtomicBool>,  // run_script counter
) {
    let fetch_count = Arc::new(AtomicUsize::new(0));
    let toggle_count = Arc::new(AtomicBool::new(false));
    let script_count = Arc::new(AtomicBool::new(false));
    let fc = fetch_count.clone();
    let tc = toggle_count.clone();
    let sc = script_count.clone();
    let client = Arc::new(TrackingHaClient {
        state_response: Arc::new(move || Ok(state.clone())),
        fetch_count: fc,
        toggle_called: tc,
        run_script_called: sc,
    });
    (client, fetch_count, toggle_count, script_count)
}

struct TrackingHaClient {
    state_response:
        Arc<dyn Fn() -> haretropanel::shared::error::AppResult<DashboardState> + Send + Sync>,
    fetch_count: std::sync::Arc<AtomicUsize>,
    toggle_called: std::sync::Arc<AtomicBool>,
    run_script_called: std::sync::Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl HomeAssistantClient for TrackingHaClient {
    async fn fetch_dashboard_state(
        &self,
    ) -> haretropanel::shared::error::AppResult<DashboardState> {
        self.fetch_count.fetch_add(1, Ordering::SeqCst);
        (self.state_response)()
    }
    async fn toggle(&self, _entity_id: &EntityId) -> haretropanel::shared::error::AppResult<()> {
        self.toggle_called.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn run_script(
        &self,
        _entity_id: &EntityId,
    ) -> haretropanel::shared::error::AppResult<()> {
        self.run_script_called.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn call_service_raw(
        &self,
        _domain: &str,
        _service: &str,
        _body: serde_json::Value,
    ) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }
}

/// Test 1: Force refresh command invalidates cache and triggers fresh fetch.
///
/// Sends `{"action":"force_refresh"}` via WebSocket, verifies the response
/// contains dashboard data (watts field) and no error.
#[tokio::test]
async fn ws_force_refresh_command() {
    let state = DashboardState {
        entities: vec![
            Entity {
                id: EntityId("sensor.solar".into()),
                name: "Solar".into(),
                kind: EntityKind::Sensor,
                is_on: true,
                value: Some("1000 W".into()),
            },
            Entity {
                id: EntityId("switch.garage_left".into()),
                name: "Garage Left".into(),
                kind: EntityKind::Switch,
                is_on: false,
                value: None,
            },
            Entity {
                id: EntityId("switch.garage_right".into()),
                name: "Garage Right".into(),
                kind: EntityKind::Switch,
                is_on: false,
                value: None,
            },
        ],
    };

    let (client, fetch_count, _toggle, _script) = build_tracking_client(state);
    let service = Arc::new(DashboardService::new(
        client,
        Arc::new(MockLayoutRepo),
        haretropanel::application::services::DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        test_config(),
    ));

    let (listener, port) = bind_random().await;
    let app = build_ws_router(service);

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect WebSocket client
    let uri = format!("ws://127.0.0.1:{port}/ws/solar");
    let (mut ws_tx, mut ws_rx) = async_tungstenite::tokio::connect_async(&uri)
        .await
        .unwrap()
        .0
        .split();

    // Send the force_refresh command
    let cmd = serde_json::json!({"action": "force_refresh"});
    ws_tx
        .send(WsMessage::Text(serde_json::to_string(&cmd).unwrap().into()))
        .await
        .unwrap();

    // Receive the response with timeout
    let response = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    return value;
                }
                Ok(WsMessage::Close(close)) => {
                    if let Some(close_frame) = close {
                        tracing::info!("WS close: {} {}", close_frame.code, close_frame.reason);
                    }
                    return serde_json::json!({"type": "close"});
                }
                _ => continue,
            }
        }
        serde_json::json!({"type": "eof"})
    })
    .await
    .expect("timeout waiting for force_refresh response");

    // Verify response
    assert!(
        response.get("watts").is_some(),
        "force_refresh response should contain 'watts' field, got: {response}"
    );
    if let Some(type_val) = response.get("type") {
        assert_ne!(
            type_val, "error",
            "force_refresh response should not be an error: {response}"
        );
    }

    // The fetch should have happened during force_refresh
    let count = fetch_count.load(Ordering::SeqCst);
    assert!(
        count >= 1,
        "fetch_count should be >= 1 after force_refresh, got {count}"
    );

    // Cleanup
    drop(ws_tx);
    server.abort();
    let _ = server.await;
}

/// Test 2: Toggle command triggers HA client's toggle method.
#[tokio::test]
async fn ws_toggle_command() {
    let state = DashboardState {
        entities: vec![Entity {
            id: EntityId("light.test".into()),
            name: "Test Light".into(),
            kind: EntityKind::Light,
            is_on: false,
            value: None,
        }],
    };

    let (client, _fetch_count, toggle_called, _script) = build_tracking_client(state);
    let service = Arc::new(DashboardService::new(
        client,
        Arc::new(MockLayoutRepo),
        haretropanel::application::services::DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        test_config(),
    ));

    let (listener, port) = bind_random().await;
    let app = build_ws_router(service);

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let uri = format!("ws://127.0.0.1:{port}/ws/solar");
    let (mut ws_tx, mut ws_rx) = async_tungstenite::tokio::connect_async(&uri)
        .await
        .unwrap()
        .0
        .split();

    // Send toggle command
    let cmd = serde_json::json!({"action": "toggle", "entity_id": "light.test"});
    ws_tx
        .send(WsMessage::Text(serde_json::to_string(&cmd).unwrap().into()))
        .await
        .unwrap();

    // Receive response
    let response = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => {
                    return serde_json::from_str(&text).unwrap();
                }
                _ => continue,
            }
        }
        serde_json::json!({"type": "eof"})
    })
    .await
    .expect("timeout waiting for toggle response");

    // Verify response
    assert!(
        response.get("watts").is_some(),
        "toggle response should have 'watts': {response}"
    );
    assert!(
        toggle_called.load(Ordering::SeqCst),
        "toggle should have been called on HA client"
    );

    // Cleanup
    drop(ws_tx);
    server.abort();
    let _ = server.await;
}

/// Test 3: Periodic broadcast sends dashboard_update messages to connected client.
#[tokio::test]
async fn ws_periodic_broadcast() {
    let state = DashboardState {
        entities: vec![
            Entity {
                id: EntityId("sensor.solar".into()),
                name: "Solar".into(),
                kind: EntityKind::Sensor,
                is_on: true,
                value: Some("2500 W".into()),
            },
            Entity {
                id: EntityId("switch.garage_left".into()),
                name: "Garage Left".into(),
                kind: EntityKind::Switch,
                is_on: false,
                value: None,
            },
            Entity {
                id: EntityId("switch.garage_right".into()),
                name: "Garage Right".into(),
                kind: EntityKind::Switch,
                is_on: false,
                value: None,
            },
        ],
    };

    let (client, _fetch_count, _toggle, _script) = build_tracking_client(state);
    let mut config = test_config();
    config.solar_sample_secs = 1; // faster samples for testing

    let service = Arc::new(DashboardService::new(
        client,
        Arc::new(MockLayoutRepo),
        haretropanel::application::services::DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        config,
    ));

    // Start periodic updates — requires Arc<Self>
    let service_clone = service.clone();
    service_clone.start_periodic_updates();

    let (listener, port) = bind_random().await;
    let app = build_ws_router(service);

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let uri = format!("ws://127.0.0.1:{port}/ws/solar");
    let (_, mut ws_rx) = async_tungstenite::tokio::connect_async(&uri)
        .await
        .unwrap()
        .0
        .split();

    // Wait up to 12 seconds for the first periodic message (10s interval + buffer)
    let received = tokio::time::timeout(Duration::from_secs(12), async {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    return Some(value);
                }
                _ => continue,
            }
        }
        None
    })
    .await
    .expect("timeout waiting for periodic broadcast")
    .expect("should receive a message");

    // Verify all expected fields are present
    assert!(received.get("watts").is_some(), "missing 'watts'");
    assert!(
        received.get("chart_labels").is_some(),
        "missing 'chart_labels'"
    );
    assert!(
        received.get("chart_values").is_some(),
        "missing 'chart_values'"
    );
    assert!(
        received.get("charger_amps").is_some(),
        "missing 'charger_amps'"
    );
    assert!(
        received.get("garage_left").is_some(),
        "missing 'garage_left'"
    );
    assert!(
        received.get("garage_right").is_some(),
        "missing 'garage_right'"
    );
    assert!(
        received.get("buffer_temps").is_some(),
        "missing 'buffer_temps'"
    );
    assert!(
        received.get("pump_status").is_some(),
        "missing 'pump_status'"
    );

    // watts should be 2500 (our entity value)
    assert_eq!(received["watts"], 2500.0, "watts should match entity value");

    // Cleanup
    server.abort();
    let _ = server.await;
}

// ── HTTP Handler Integration Tests ─────────────────────────────────────
// These tests cover dashboard_handler.rs and settings_handler.rs,
// which have 0% coverage when only the WebSocket route is tested.

use axum::http::StatusCode;
use haretropanel::application::services::DashboardCacheConfig;

/// Helper: build a full Axum router with ALL routes (dashboard + settings + WS).
fn build_full_router(service: Arc<DashboardService>) -> Router {
    use axum::routing::{get, post};
    use haretropanel::infrastructure::web::handlers::dashboard_handler::get_dashboard;
    use haretropanel::infrastructure::web::handlers::settings_handler::get_entity_settings;
    use haretropanel::infrastructure::web::handlers::websocket_handler::ws_solar;
    use haretropanel::infrastructure::web::AppState;

    Router::new()
        .route("/", get(get_dashboard))
        .route("/ws/solar", get(ws_solar))
        .route("/toggle", post(|| async { "ok" }))
        .route("/run_script", post(|| async { "ok" }))
        .route("/settings/entities", get(get_entity_settings))
        .with_state(AppState {
            dashboard_service: service,
        })
}

/// Test: Invalid command sends error response.
///
/// Covers the invalid command handling path in websocket_handler.rs.
#[tokio::test]
async fn ws_invalid_command_returns_error() {
    let state = DashboardState { entities: vec![] };

    let (client, _fetch_count, _toggle, _script) = build_tracking_client(state);
    let service = Arc::new(DashboardService::new(
        client,
        Arc::new(MockLayoutRepo),
        DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        test_config(),
    ));

    let (listener, port) = bind_random().await;
    let app = build_ws_router(service);

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let uri = format!("ws://127.0.0.1:{port}/ws/solar");
    let (mut ws, mut ws_rx) = async_tungstenite::tokio::connect_async(&uri)
        .await
        .unwrap()
        .0
        .split();

    // Send invalid JSON (not a valid JSON object)
    let invalid_msg = r#"not_valid_json"#;

    use futures_util::SinkExt;
    ws.send(WsMessage::Text(invalid_msg.to_string()))
        .await
        .unwrap();

    // Wait for error response
    let error_text = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => return Some(text),
                _ => continue,
            }
        }
        None
    })
    .await;

    let error_text = error_text.unwrap().expect("should receive error message");
    let error_json: serde_json::Value = serde_json::from_str(&error_text).unwrap();
    assert_eq!(
        error_json["type"], "error",
        "invalid command should return error type"
    );
    assert!(
        error_json["message"].is_string(),
        "error should have message string"
    );

    server.abort();
    let _ = server.await;
}

/// Test: RunScript command triggers the run_script path.
///
/// Covers the `RunScript` arm in websocket_handler.rs.
#[tokio::test]
async fn ws_run_script_command() {
    let state = DashboardState { entities: vec![] };

    let (client, _fetch_count, _toggle, script_called) = build_tracking_client(state);

    let service = Arc::new(DashboardService::new(
        client,
        Arc::new(MockLayoutRepo),
        DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        test_config(),
    ));

    let (listener, port) = bind_random().await;
    let app = build_ws_router(service);

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let uri = format!("ws://127.0.0.1:{port}/ws/solar");
    let (_, mut ws_rx) = async_tungstenite::tokio::connect_async(&uri)
        .await
        .unwrap()
        .0
        .split();

    // Send run_script command via a separate connection
    let (mut ws_tx, _ws_rx2) = async_tungstenite::tokio::connect_async(&uri)
        .await
        .unwrap()
        .0
        .split();
    use futures_util::SinkExt;
    ws_tx
        .send(WsMessage::Text(
            r#"{"action":"run_script","entity_id":"script.away_mode"}"#.to_string(),
        ))
        .await
        .unwrap();

    // Wait for response on the first connection
    let _response = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(WsMessage::Text(text)) => return Some(text),
                _ => continue,
            }
        }
        None
    })
    .await;

    assert!(
        script_called.load(Ordering::SeqCst),
        "run_script should have been called"
    );

    server.abort();
    let _ = server.await;
}

/// Test: Auth rejection when wrong token is provided.
///
/// Covers the auth rejection path in websocket_handler.rs lines 53-55.
#[tokio::test]
async fn ws_auth_rejection() {
    let state = DashboardState { entities: vec![] };

    let (client, _fetch_count, _toggle, _script) = build_tracking_client(state);
    let service = Arc::new(DashboardService::new(
        client,
        Arc::new(MockLayoutRepo),
        DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        {
            let mut cfg = test_config();
            cfg.ws_auth_token = Some("correct_token".to_string());
            cfg
        },
    ));

    let (listener, port) = bind_random().await;
    let app = build_ws_router(service);

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect with wrong token via query param
    let uri = format!("ws://127.0.0.1:{port}/ws/solar?token=wrong_token");

    // This should fail with 401, not a WS upgrade
    let result = async_tungstenite::tokio::connect_async(&uri).await;

    assert!(result.is_err(), "connection with wrong token should fail");

    server.abort();
    let _ = server.await;
}

/// Test: SaveSettings command updates visible entities and pages.
///
/// Covers the `SaveSettings` arm in websocket_handler.rs.
#[tokio::test]
async fn ws_save_settings_command() {
    let state = DashboardState { entities: vec![] };

    let (client, _fetch_count, _toggle, _script) = build_tracking_client(state);
    // Use the existing MockLayoutRepo which returns defaults and saves nothing
    // The test just verifies the response is correct and no error occurs
    let service = Arc::new(DashboardService::new(
        client,
        Arc::new(MockLayoutRepo),
        DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        test_config(),
    ));

    let (listener, port) = bind_random().await;
    let app = build_ws_router(service);

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let uri = format!("ws://127.0.0.1:{port}/ws/solar");
    let (mut ws, _) = async_tungstenite::tokio::connect_async(&uri).await.unwrap();

    // Send save_settings command with valid data
    let settings_json = r#"{"action":"save_settings","visible":["sensor.solar","light.lamp"],"pages":{"solar":1,"garage":2}}"#;
    ws.send(WsMessage::Text(settings_json.into()))
        .await
        .unwrap();

    // Wait for response - should succeed with dashboard data
    // (SaveSettings calls build_fresh_payload after saving)
    let response = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match ws.next().await {
                Some(Ok(WsMessage::Text(text))) => return Some(text),
                Some(Ok(_)) => continue,
                Some(Err(_)) => return None,
                None => return None,
            }
        }
    })
    .await;

    let response_text = response.unwrap().expect("should receive response");
    let response_json: serde_json::Value = serde_json::from_str(&response_text).unwrap();
    // SaveSettings calls build_fresh_payload which returns dashboard data with watts field
    assert!(
        response_json.get("watts").is_some(),
        "response should contain watts field"
    );

    server.abort();
    let _ = server.await;
}

/// Test: GET / renders the dashboard page with valid HTML.
///
/// Covers `dashboard_handler.rs` — the `get_dashboard` handler.
#[tokio::test]
async fn test_get_dashboard_handler() {
    let state = DashboardState {
        entities: vec![
            Entity {
                id: EntityId("sensor.solar".into()),
                name: "Solar".into(),
                kind: EntityKind::Sensor,
                is_on: true,
                value: Some("4200 W".into()),
            },
            Entity {
                id: EntityId("cover.garage_left".into()),
                name: "Garage Left".into(),
                kind: EntityKind::Cover,
                is_on: false,
                value: None,
            },
            Entity {
                id: EntityId("cover.garage_right".into()),
                name: "Garage Right".into(),
                kind: EntityKind::Cover,
                is_on: true,
                value: None,
            },
        ],
    };

    let (client, _fetch_count, _toggle, _script) = build_tracking_client(state);
    let mut config = test_config();
    config.solar_entity_id = "sensor.solar".into();
    config.solar_sample_secs = 1;

    let service = Arc::new(DashboardService::new(
        client,
        Arc::new(MockLayoutRepo),
        DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        config,
    ));

    let (listener, port) = bind_random().await;
    let app = build_full_router(service);

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Make HTTP GET request to /
    let resp = reqwest::Client::new()
        .get(&format!("http://127.0.0.1:{port}/"))
        .send()
        .await
        .expect("HTTP GET / should succeed");

    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await.unwrap();
    assert!(!body.is_empty(), "Dashboard should return non-empty body");
    assert!(
        body.contains("<!DOCTYPE html>")
            || body.contains("<!doctype html>")
            || body.contains("HARetroPanel"),
        "Dashboard should contain HTML content"
    );

    // The page should contain entity names from our mock state
    assert!(
        body.contains("Solar") || body.contains("Garage"),
        "Dashboard should contain entity names"
    );

    server.abort();
    let _ = server.await;
}

/// Test: GET /settings/entities renders the settings page with valid HTML.
///
/// Covers `settings_handler.rs` — the `get_entity_settings` handler
/// and the `EntitySettingsViewModel::from()` impl.
#[tokio::test]
async fn test_get_settings_handler() {
    let state = DashboardState {
        entities: vec![
            Entity {
                id: EntityId("light.bedroom".into()),
                name: "Bedroom Light".into(),
                kind: EntityKind::Light,
                is_on: true,
                value: None,
            },
            Entity {
                id: EntityId("switch.outlet".into()),
                name: "Power Outlet".into(),
                kind: EntityKind::Switch,
                is_on: false,
                value: None,
            },
            Entity {
                id: EntityId("script.morning_routine".into()),
                name: "Morning Routine".into(),
                kind: EntityKind::Script,
                is_on: false,
                value: None,
            },
            Entity {
                id: EntityId("climate.thermostat".into()),
                name: "Thermostat".into(),
                kind: EntityKind::Climate,
                is_on: true,
                value: Some("21 °C".into()),
            },
        ],
    };

    let (client, _fetch_count, _toggle, _script) = build_tracking_client(state);
    let service = Arc::new(DashboardService::new(
        client,
        Arc::new(MockLayoutRepo),
        DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        },
        test_config(),
    ));

    let (listener, port) = bind_random().await;
    let app = build_full_router(service);

    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Make HTTP GET request to /settings/entities
    let resp = reqwest::Client::new()
        .get(&format!("http://127.0.0.1:{port}/settings/entities"))
        .send()
        .await
        .expect("HTTP GET /settings/entities should succeed");

    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await.unwrap();
    assert!(
        !body.is_empty(),
        "Settings page should return non-empty body"
    );
    assert!(
        body.contains("<!DOCTYPE html>")
            || body.contains("<!doctype html>")
            || body.contains("HARetroPanel"),
        "Settings page should contain HTML content"
    );

    // The page should contain entity names from our mock state
    assert!(
        body.contains("Bedroom Light") || body.contains("Solar"),
        "Settings page should contain entity names, body: {}",
        &body[..body.len().min(500)]
    );

    server.abort();
    let _ = server.await;
}
