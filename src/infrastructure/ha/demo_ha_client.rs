use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::{
    application::ports::HomeAssistantClient,
    config::AppConfig,
    domain::{DashboardState, Entity, EntityId, EntityKind},
    shared::error::AppResult,
};

pub struct DemoHaClient {
    state: RwLock<DashboardState>,
    demo_energy_kwh: RwLock<f64>,
    last_energy_update: RwLock<std::time::SystemTime>,
    // Maps action entity ID (e.g. "cover.garage_garagentor_links_bewegen")
    // to the index of the status entity in the state entities vec.
    garage_action_to_status: RwLock<std::collections::HashMap<String, usize>>,
}

impl DemoHaClient {
    pub fn new(config: &AppConfig) -> Arc<Self> {
        let solar = Entity {
            id: EntityId(config.solar_entity_id.clone()),
            name: "Solar Production".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("4200 W".to_string()),
        };

        let solar_peak = Entity {
            id: EntityId("sensor.solar_peak".to_string()),
            name: "Solar Peak".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("7800 W".to_string()),
        };

        let buffer_top = Entity {
            id: EntityId(config.solar_buffer_top_entity_id.clone()),
            name: "Buffer Top".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("55.2 °C".to_string()),
        };

        let buffer_bottom = Entity {
            id: EntityId(config.solar_buffer_bottom_entity_id.clone()),
            name: "Buffer Bottom".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("48.5 °C".to_string()),
        };

        let solar_flow = Entity {
            id: EntityId(config.solar_flow_entity_id.clone()),
            name: "Solar Flow Pre-Panel".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("52.0 °C".to_string()),
        };

        let solar_return = Entity {
            id: EntityId(config.solar_return_entity_id.clone()),
            name: "Solar Return".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("45.0 °C".to_string()),
        };

        let solar_pump = Entity {
            id: EntityId(config.solar_pump_entity_id.clone()),
            name: "Solar Pump".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("on".to_string()),
        };

        let charger = Entity {
            id: EntityId(config.charger_current_entity_id.clone()),
            name: "Go-E Charger Current".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("0 W".to_string()),
        };

        let goe_status = Entity {
            id: EntityId(config.goe_status_entity_id.clone()),
            name: "Go-E Status".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("Nicht laden".to_string()),
        };

        let goe_energy = Entity {
            id: EntityId(config.goe_energy_entity_id.clone()),
            name: "Lademenge seit Anstöpseln".to_string(),
            kind: EntityKind::Sensor,
            is_on: false,
            value: Some("0.8 kWh".to_string()),
        };

        let goe_car = Entity {
            id: EntityId(config.goe_car_connected_entity_id.clone()),
            name: "Car Connected".to_string(),
            kind: EntityKind::Sensor,
            is_on: true,
            value: Some("on".to_string()),
        };

        let garage_left = Entity {
            id: EntityId(config.garage_left_status_entity_id.clone()),
            name: "Garage Left".to_string(),
            kind: EntityKind::Cover,
            is_on: false,
            value: Some("closed".to_string()),
        };

        let garage_right = Entity {
            id: EntityId(config.garage_right_status_entity_id.clone()),
            name: "Garage Right".to_string(),
            kind: EntityKind::Cover,
            is_on: true,
            value: Some("open".to_string()),
        };

        let state = DashboardState {
            entities: vec![
                solar,
                solar_peak,
                buffer_top,
                buffer_bottom,
                solar_flow,
                solar_return,
                solar_pump,
                charger,
                goe_status,
                goe_energy,
                goe_car,
                garage_left,
                garage_right,
            ],
        };

        // Build action → status entity index mapping
        let mut action_to_status = std::collections::HashMap::new();
        if !config.garage_left_action_entity_id.is_empty()
            && !config.garage_left_status_entity_id.is_empty()
        {
            // Find status entity index
            if let Some(status_idx) = state
                .entities
                .iter()
                .position(|e| e.id.0 == config.garage_left_status_entity_id)
            {
                action_to_status.insert(
                    config.garage_left_action_entity_id.clone(),
                    status_idx,
                );
            }
        }
        if !config.garage_right_action_entity_id.is_empty()
            && !config.garage_right_status_entity_id.is_empty()
        {
            if let Some(status_idx) = state
                .entities
                .iter()
                .position(|e| e.id.0 == config.garage_right_status_entity_id)
            {
                action_to_status.insert(
                    config.garage_right_action_entity_id.clone(),
                    status_idx,
                );
            }
        }

        Arc::new(Self {
            state: RwLock::new(state),
            demo_energy_kwh: RwLock::new(0.8),
            last_energy_update: RwLock::new(std::time::SystemTime::now()),
            garage_action_to_status: RwLock::new(action_to_status),
        })
    }

    async fn toggle_entity_state(&self, entity_id: &EntityId) {
        let mut state = self.state.write().await;
        let mut toggled = false;
        for entity in &mut state.entities {
            if &entity.id == entity_id {
                if entity.kind == EntityKind::Cover {
                    entity.is_on = !entity.is_on;
                    entity.value = Some(if entity.is_on { "open" } else { "closed" }.to_string());
                }
                toggled = true;
                break;
            }
        }
        // If not found directly, try mapping from action entity ID → status entity
        if !toggled {
            let mapping = self.garage_action_to_status.read().await;
            if let Some(status_idx) = mapping.get(&entity_id.0) {
                let entity = &mut state.entities[*status_idx];
                entity.is_on = !entity.is_on;
                entity.value = Some(if entity.is_on { "open" } else { "closed" }.to_string());
            }
        }
    }
}

#[async_trait]
impl HomeAssistantClient for DemoHaClient {
    async fn fetch_dashboard_state(&self) -> AppResult<DashboardState> {
        let now = std::time::SystemTime::now();
        let mut last_update = self.last_energy_update.write().await;
        if let Ok(elapsed) = now.duration_since(*last_update) {
            if elapsed.as_secs() >= 10 {
                let mut energy = self.demo_energy_kwh.write().await;
                *energy += 0.05;
                *last_update = now;
            }
        }

        let mut state = self.state.write().await;
        let energy_value = *self.demo_energy_kwh.read().await;
        for entity in &mut state.entities {
            if entity.id.0.contains("goe") && entity.name.contains("Lademenge") {
                entity.value = Some(format!("{:.2} kWh", energy_value));
            }
        }
        Ok(state.clone())
    }

    async fn toggle(&self, entity_id: &EntityId) -> AppResult<()> {
        self.toggle_entity_state(entity_id).await;
        Ok(())
    }

    async fn run_script(&self, _entity_id: &EntityId) -> AppResult<()> {
        Ok(())
    }
}
