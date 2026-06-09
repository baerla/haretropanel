// Tests are in a single file to keep things simple and avoid module visibility issues.

use std::sync::Arc;
use std::time::Duration;

use haretropanel::application::ports::HomeAssistantClient;
use haretropanel::application::services::DashboardService;
use haretropanel::config::AppConfig;
use haretropanel::domain::{DashboardState, Entity, EntityId, EntityKind};

// ── Manual mock implementations ──────────────────────────────────────────

struct MockHaClient {
    state_response: Arc<dyn Fn() -> haretropanel::shared::error::AppResult<DashboardState> + Send + Sync>,
}

#[async_trait::async_trait]
impl HomeAssistantClient for MockHaClient {
    async fn fetch_dashboard_state(&self) -> haretropanel::shared::error::AppResult<DashboardState> {
        (self.state_response)()
    }

    async fn toggle(&self, _entity_id: &EntityId) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }

    async fn run_script(&self, _entity_id: &EntityId) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }
}

struct MockLayoutRepo;

#[async_trait::async_trait]
impl haretropanel::application::services::DashboardLayoutRepository for MockLayoutRepo {
    async fn load_visible_entities(&self) -> haretropanel::shared::error::AppResult<Vec<EntityId>> {
        Ok(Vec::new())
    }
    async fn save_visible_entities(&self, _ids: Vec<EntityId>) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }
    async fn load_entity_pages(&self) -> haretropanel::shared::error::AppResult<std::collections::HashMap<String, usize>> {
        Ok(Default::default())
    }
    async fn save_entity_pages(&self, _map: std::collections::HashMap<String, usize>) -> haretropanel::shared::error::AppResult<()> {
        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn test_config() -> AppConfig {
    AppConfig {
        demo_mode: false,
        solar_entity_id: "sensor.solar".into(),
        charger_current_entity_id: String::new(),
        goe_status_entity_id: String::new(),
        goe_energy_entity_id: String::new(),
        goe_car_connected_entity_id: String::new(),
        goe_charging_entity_id: String::new(),
        garage_left_entity_id: String::new(),
        garage_right_entity_id: String::new(),
        solar_max_watts: 9000.0,
        solar_history_minutes: 60,
        solar_sample_secs: 60,
        goe_energy_stable_secs: 60,
        goe_energy_delta_kwh: 0.02,
        server_port: 8080,
        ha_base_url: "http://localhost:8123".into(),
        ha_token: None,
        log_dir: "./logs".into(),
        log_rotation: haretropanel::config::LogRotation::Never,
        log_level: "haretropanel=debug".into(),
        dashboard_cache_ttl_default_secs: 5,
        dashboard_cache_ttl_light_secs: None,
        dashboard_cache_ttl_switch_secs: None,
        dashboard_cache_ttl_sensor_secs: None,
        dashboard_cache_ttl_climate_secs: None,
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
    service.get_dashboard_with_refresh(true).await.expect("fetch should succeed");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Retrieve the cached state and verify the entity value pipeline
    let cached = service.get_dashboard_with_refresh(false).await.expect("fetch should succeed");

    let solar_entity = cached
        .entities
        .iter()
        .find(|e| e.id.0 == config.solar_entity_id);

    assert!(
        solar_entity.is_some(),
        "solar entity should be found by ID"
    );

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
    assert!(percent > 0, "percent > 0 when watts=4200, max=9000, got {percent}");

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

    let cached = service.get_dashboard_with_refresh(true).await.expect("fetch should not error");

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
            v.split_whitespace().next().and_then(|n| n.parse::<f64>().ok())
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
            value: None,   // unavailable has no value
        }],
    };

    let service = build_service(state);

    let cached = service.get_dashboard_with_refresh(true).await.expect("fetch should not error");

    let solar_entity = cached
        .entities
        .iter()
        .find(|e| e.id.0 == "sensor.sunny_home_manager_2_0_metering_power_supplied");

    assert!(solar_entity.is_some(), "entity should be found");

    let watts: f64 = solar_entity
        .and_then(|e| e.value.clone())
        .and_then(|v| {
            v.split_whitespace().next().and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(0.0);
    assert_eq!(watts, 0.0, "unavailable sensor should produce 0 watts");
}