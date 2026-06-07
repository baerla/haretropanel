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
            id: EntityId(config.garage_left_entity_id.clone()),
            name: "Garage Left".to_string(),
            kind: EntityKind::Cover,
            is_on: false,
            value: Some("closed".to_string()),
        };

        let garage_right = Entity {
            id: EntityId(config.garage_right_entity_id.clone()),
            name: "Garage Right".to_string(),
            kind: EntityKind::Cover,
            is_on: true,
            value: Some("open".to_string()),
        };

        let state = DashboardState {
            entities: vec![
                solar,
                solar_peak,
                charger,
                goe_status,
                goe_energy,
                goe_car,
                garage_left,
                garage_right,
            ],
        };

        Arc::new(Self {
            state: RwLock::new(state),
            demo_energy_kwh: RwLock::new(0.8),
            last_energy_update: RwLock::new(std::time::SystemTime::now()),
        })
    }

    async fn toggle_entity_state(&self, entity_id: &EntityId) {
        let mut state = self.state.write().await;
        for entity in &mut state.entities {
            if &entity.id == entity_id {
                if entity.kind == EntityKind::Cover {
                    entity.is_on = !entity.is_on;
                    entity.value = Some(if entity.is_on { "open" } else { "closed" }.to_string());
                }
                break;
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
