use std::sync::Arc;

use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION},
    Client, Url,
};

use crate::{
    application::ports::HomeAssistantClient,
    config::AppConfig,
    domain::{DashboardState, Entity, EntityId, EntityKind},
    infrastructure::ha::ha_models::HaStateResponse,
    shared::error::{AppError, AppResult},
};

pub struct HaHttpClient {
    config: AppConfig,
    client: Client,
}

pub fn entity_kind_from_id(entity_id: &str) -> EntityKind {
    if entity_id.starts_with("light.") {
        EntityKind::Light
    } else if entity_id.starts_with("switch.") {
        EntityKind::Switch
    } else if entity_id.starts_with("climate.") {
        EntityKind::Climate
    } else if entity_id.starts_with("script.") {
        EntityKind::Script
    } else {
        EntityKind::Sensor
    }
}

pub fn is_on(kind: &EntityKind, state: &str) -> bool {
    match kind {
        EntityKind::Light | EntityKind::Switch => state == "on",
        EntityKind::Climate => matches!(state, "heat" | "cool" | "heat_cool" | "auto"),
        EntityKind::Sensor => matches!(state, "on" | "open" | "home" | "above_horizon"),
        EntityKind::Script => state == "on",
        EntityKind::Cover => matches!(state, "open" | "opening"),
    }
}

pub fn build_value(kind: &EntityKind, state: &str, ha: &HaStateResponse) -> Option<String> {
    match kind {
        EntityKind::Sensor | EntityKind::Climate => {
            if let Some(unit) = &ha.attributes.unit_of_measurement {
                Some(format!("{state} {unit}"))
            } else {
                Some(state.to_string())
            }
        }
        EntityKind::Script => Some(state.to_string()),
        EntityKind::Light | EntityKind::Switch => None,
        EntityKind::Cover => {
            if let Some(unit) = &ha.attributes.unit_of_measurement {
                Some(format!("{state} {unit}"))
            } else {
                Some(state.to_string())
            }
        }
    }
}

impl HaHttpClient {
    pub fn new(config: AppConfig) -> AppResult<Arc<Self>> {
        let client = Self::build_client(&config)?;
        Ok(Arc::new(Self { config, client }))
    }

    fn build_client(config: &AppConfig) -> AppResult<Client> {
        let mut headers = HeaderMap::new();

        let token = config
            .ha_token
            .as_ref()
            .ok_or_else(|| AppError::Internal("Missing Home Assistant token (HA_TOKEN)".into()))?;

        let value = HeaderValue::from_str(&format!("Bearer {}", token))
            .map_err(|_| AppError::Internal("Invalid Home Assistant token".into()))?;

        headers.insert(AUTHORIZATION, value);

        Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| AppError::Internal(format!("Failed to build HTTP client: {e}")))
    }

    fn base_url(&self) -> AppResult<Url> {
        Url::parse(&self.config.ha_base_url)
            .map_err(|e| AppError::Internal(format!("Invalid HA_BASE_URL: {e}")))
    }

    fn map_entity_kind(entity_id: &str) -> EntityKind {
        if entity_id.starts_with("light.") {
            EntityKind::Light
        } else if entity_id.starts_with("switch.") {
            EntityKind::Switch
        } else if entity_id.starts_with("climate.") {
            EntityKind::Climate
        } else if entity_id.starts_with("script.") {
            EntityKind::Script
        } else if entity_id.starts_with("cover.") {
            EntityKind::Cover
        } else {
            EntityKind::Sensor
        }
    }

    fn is_on_state(kind: &EntityKind, state: &str) -> bool {
        match kind {
            EntityKind::Light | EntityKind::Switch => state == "on",
            EntityKind::Climate => matches!(state, "heat" | "cool" | "heat_cool" | "auto"),
            EntityKind::Sensor => matches!(state, "on" | "open" | "home" | "above_horizon"),
            EntityKind::Script => state == "on",
            EntityKind::Cover => matches!(state, "open" | "opening"),
        }
    }

    fn build_value(kind: &EntityKind, state: &str, ha: &HaStateResponse) -> Option<String> {
        match kind {
            EntityKind::Sensor | EntityKind::Climate | EntityKind::Cover => {
                if let Some(unit) = &ha.attributes.unit_of_measurement {
                    Some(format!("{state} {unit}"))
                } else {
                    Some(state.to_string())
                }
            }
            EntityKind::Script => Some(state.to_string()),
            EntityKind::Light | EntityKind::Switch => None,
        }
    }

    fn build_entity(ha: HaStateResponse) -> Entity {
        let kind = entity_kind_from_id(&ha.entity_id);

        let name = ha
            .attributes
            .friendly_name
            .clone()
            .unwrap_or_else(|| ha.entity_id.clone());

        let is_on = is_on(&kind, &ha.state);
        let value = build_value(&kind, &ha.state, &ha);

        tracing::debug!(
            entity_id = &ha.entity_id,
            raw_state = &ha.state,
            name = &name,
            kind = ?kind,
            is_on,
            value = ?value,
            "Parsed HA state response"
        );

        Entity {
            id: EntityId(ha.entity_id),
            name,
            kind,
            is_on,
            value,
        }
    }

    async fn call_service(
        &self,
        domain: &str,
        service: &str,
        body: serde_json::Value,
    ) -> AppResult<()> {
        let mut url = self.base_url()?;
        url.set_path(&format!("api/services/{domain}/{service}"));

        tracing::debug!(service_url = url.as_str(), "Sending HA service call");

        let resp = self
            .client
            .post(url.clone())
            .json(&body)
            .send()
            .await
            .map_err(AppError::Http)?;

        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "HA service call {} failed with status {}",
                url,
                resp.status()
            )));
        }

        tracing::debug!(service_url = url.as_str(), "HA service call succeeded");

        Ok(())
    }
}

#[async_trait]
impl HomeAssistantClient for HaHttpClient {
    async fn fetch_dashboard_state(&self) -> AppResult<DashboardState> {
        let mut url = self.base_url()?;
        url.set_path("api/states");

        tracing::debug!(ha_url = url.as_str(), "Requesting all HA entity states");

        let resp = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(AppError::Http)?;

        if !resp.status().is_success() {
            return Err(AppError::Internal(format!(
                "HA GET {} failed with status {}",
                url,
                resp.status()
            )));
        }

        let ha_states = resp
            .json::<Vec<HaStateResponse>>()
            .await
            .map_err(AppError::Http)?;

        let entity_count = ha_states.len();
        let entities = ha_states.into_iter().map(Self::build_entity).collect();

        tracing::debug!(entity_count, "Fetched HA entity states");

        Ok(DashboardState { entities })
    }

    async fn toggle(&self, entity_id: &EntityId) -> AppResult<()> {
        let id_str = &entity_id.0;

        let domain = id_str
            .split('.')
            .next()
            .ok_or_else(|| AppError::Internal(format!("Invalid entity_id: {id_str}")))?;

        let body = serde_json::json!({ "entity_id": id_str });

        tracing::debug!(entity_id = %id_str, domain, "Calling HA toggle service");
        self.call_service(domain, "toggle", body).await
    }

    async fn run_script(&self, entity_id: &EntityId) -> AppResult<()> {
        let id_str = &entity_id.0;

        if !id_str.starts_with("script.") {
            return Err(AppError::Internal(format!(
                "run_script called with non-script entity_id: {id_str}"
            )));
        }

        let body = serde_json::json!({ "entity_id": id_str });

        tracing::debug!(entity_id = %id_str, "Calling HA script turn_on service");
        self.call_service("script", "turn_on", body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, LogRotation};

    #[test]
    fn test_entity_kind_from_id_light() {
        assert_eq!(entity_kind_from_id("light.lamp"), EntityKind::Light);
    }

    #[test]
    fn test_entity_kind_from_id_switch() {
        assert_eq!(entity_kind_from_id("switch.outdoor"), EntityKind::Switch);
    }

    #[test]
    fn test_entity_kind_from_id_climate() {
        assert_eq!(entity_kind_from_id("climate.home"), EntityKind::Climate);
    }

    #[test]
    fn test_entity_kind_from_id_script() {
        assert_eq!(entity_kind_from_id("script.away_mode"), EntityKind::Script);
    }

    #[test]
    fn test_entity_kind_from_id_sensor_fallback() {
        assert_eq!(entity_kind_from_id("binary_sensor.motion"), EntityKind::Sensor);
        assert_eq!(entity_kind_from_id("zone.home"), EntityKind::Sensor);
        assert_eq!(entity_kind_from_id("unknown.domain"), EntityKind::Sensor);
    }

    #[test]
    fn test_is_on_light_on() {
        assert!(is_on(&EntityKind::Light, "on"));
    }

    #[test]
    fn test_is_on_light_off() {
        assert!(!is_on(&EntityKind::Light, "off"));
    }

    #[test]
    fn test_is_on_switch_on() {
        assert!(is_on(&EntityKind::Switch, "on"));
    }

    #[test]
    fn test_is_on_switch_off() {
        assert!(!is_on(&EntityKind::Switch, "off"));
    }

    #[test]
    fn test_is_on_climate_heat() {
        assert!(is_on(&EntityKind::Climate, "heat"));
        assert!(is_on(&EntityKind::Climate, "cool"));
        assert!(is_on(&EntityKind::Climate, "heat_cool"));
        assert!(is_on(&EntityKind::Climate, "auto"));
    }

    #[test]
    fn test_is_on_climate_not_on() {
        assert!(!is_on(&EntityKind::Climate, "off"));
        assert!(!is_on(&EntityKind::Climate, "idle"));
    }

    #[test]
    fn test_is_on_sensor_on() {
        assert!(is_on(&EntityKind::Sensor, "on"));
        assert!(is_on(&EntityKind::Sensor, "open"));
        assert!(is_on(&EntityKind::Sensor, "home"));
        assert!(is_on(&EntityKind::Sensor, "above_horizon"));
    }

    #[test]
    fn test_is_on_sensor_off() {
        assert!(!is_on(&EntityKind::Sensor, "off"));
        assert!(!is_on(&EntityKind::Sensor, "closed"));
        assert!(!is_on(&EntityKind::Sensor, "not_home"));
        assert!(!is_on(&EntityKind::Sensor, "below_horizon"));
    }

    #[test]
    fn test_is_on_script_on() {
        assert!(is_on(&EntityKind::Script, "on"));
    }

    #[test]
    fn test_is_on_script_off() {
        assert!(!is_on(&EntityKind::Script, "off"));
    }

    #[test]
    fn test_build_value_sensor_no_unit() {
        let ha = HaStateResponse {
            entity_id: "sensor.temp".to_string(),
            state: "22".to_string(),
            attributes: crate::infrastructure::ha::ha_models::HaAttributes {
                friendly_name: None,
                unit_of_measurement: None,
            },
        };
        let val = build_value(&EntityKind::Sensor, "22", &ha);
        assert_eq!(val, Some("22".to_string()));
    }

    #[test]
    fn test_build_value_sensor_with_unit() {
        let ha = HaStateResponse {
            entity_id: "sensor.temp".to_string(),
            state: "22".to_string(),
            attributes: crate::infrastructure::ha::ha_models::HaAttributes {
                friendly_name: None,
                unit_of_measurement: Some("°C".to_string()),
            },
        };
        let val = build_value(&EntityKind::Sensor, "22", &ha);
        assert_eq!(val, Some("22 °C".to_string()));
    }

    #[test]
    fn test_build_value_climate_with_unit() {
        let ha = HaStateResponse {
            entity_id: "climate.home".to_string(),
            state: "24".to_string(),
            attributes: crate::infrastructure::ha::ha_models::HaAttributes {
                friendly_name: None,
                unit_of_measurement: Some("°F".to_string()),
            },
        };
        let val = build_value(&EntityKind::Climate, "24", &ha);
        assert_eq!(val, Some("24 °F".to_string()));
    }

    #[test]
    fn test_build_value_script() {
        let ha = HaStateResponse {
            entity_id: "script.away".to_string(),
            state: "on".to_string(),
            attributes: crate::infrastructure::ha::ha_models::HaAttributes {
                friendly_name: None,
                unit_of_measurement: None,
            },
        };
        let val = build_value(&EntityKind::Script, "on", &ha);
        assert_eq!(val, Some("on".to_string()));
    }

    #[test]
    fn test_build_value_light_is_none() {
        let ha = HaStateResponse {
            entity_id: "light.lamp".to_string(),
            state: "on".to_string(),
            attributes: crate::infrastructure::ha::ha_models::HaAttributes {
                friendly_name: None,
                unit_of_measurement: None,
            },
        };
        assert!(build_value(&EntityKind::Light, "on", &ha).is_none());
    }

    #[test]
    fn test_build_value_switch_is_none() {
        let ha = HaStateResponse {
            entity_id: "switch.outdoor".to_string(),
            state: "on".to_string(),
            attributes: crate::infrastructure::ha::ha_models::HaAttributes {
                friendly_name: None,
                unit_of_measurement: None,
            },
        };
        assert!(build_value(&EntityKind::Switch, "on", &ha).is_none());
    }

    fn base_config() -> AppConfig {
        AppConfig {
            server_port: 8080,
            ha_base_url: "http://localhost:8123".to_string(),
            ha_token: Some("token".to_string()),
            log_dir: "./logs".to_string(),
            log_rotation: LogRotation::Daily,
            log_level: "info".to_string(),
            dashboard_cache_ttl_default_secs: 5,
            dashboard_cache_ttl_light_secs: None,
            dashboard_cache_ttl_switch_secs: None,
            dashboard_cache_ttl_sensor_secs: None,
            dashboard_cache_ttl_climate_secs: None,
            charger_current_entity_id: "".to_string(),
            demo_mode: false,
            garage_left_status_entity_id: "".to_string(),
            garage_left_action_entity_id: "".to_string(),
            garage_right_status_entity_id: "".to_string(),
            garage_right_action_entity_id: "".to_string(),
            goe_car_connected_entity_id: "".to_string(),
            goe_charging_entity_id: "".to_string(),
            goe_energy_delta_kwh: 0.0,
            goe_energy_stable_secs: 60,
            goe_energy_entity_id: "".to_string(),
            goe_status_entity_id: "".to_string(),
            solar_entity_id: "".to_string(),
            solar_history_minutes: 3600,
            solar_max_watts: 0.0,
            solar_sample_secs: 60,

        }
    }

    #[test]
    fn test_build_entity_prefers_friendly_name() {
        let ha = HaStateResponse {
            entity_id: "light.lamp".to_string(),
            state: "on".to_string(),
            attributes: crate::infrastructure::ha::ha_models::HaAttributes {
                friendly_name: Some("Living Room".to_string()),
                unit_of_measurement: None,
            },
        };

        let entity = HaHttpClient::build_entity(ha);

        assert_eq!(entity.name, "Living Room");
        assert_eq!(entity.kind, EntityKind::Light);
        assert!(entity.is_on);
        assert!(entity.value.is_none());
    }

    #[test]
    fn test_build_entity_fallbacks_to_entity_id() {
        let ha = HaStateResponse {
            entity_id: "sensor.temp".to_string(),
            state: "21".to_string(),
            attributes: crate::infrastructure::ha::ha_models::HaAttributes {
                friendly_name: None,
                unit_of_measurement: Some("°C".to_string()),
            },
        };

        let entity = HaHttpClient::build_entity(ha);

        assert_eq!(entity.name, "sensor.temp");
        assert_eq!(entity.kind, EntityKind::Sensor);
        assert_eq!(entity.value, Some("21 °C".to_string()));
    }

    #[test]
    fn test_build_client_requires_token() {
        let mut config = base_config();
        config.ha_token = None;

        let result = HaHttpClient::build_client(&config);

        assert!(result.is_err());
    }

    #[test]
    fn test_build_client_rejects_invalid_token() {
        let mut config = base_config();
        config.ha_token = Some("bad\ntoken".to_string());

        let result = HaHttpClient::build_client(&config);

        assert!(result.is_err());
    }

    #[test]
    fn test_base_url_rejects_invalid_url() {
        let mut config = base_config();
        config.ha_base_url = "not a url".to_string();
        let client = HaHttpClient {
            config,
            client: Client::new(),
        };

        let result = client.base_url();

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_script_rejects_non_script() {
        let client = HaHttpClient {
            config: base_config(),
            client: Client::new(),
        };

        let result = client
            .run_script(&EntityId("light.lamp".to_string()))
            .await;

        assert!(result.is_err());
    }
}
