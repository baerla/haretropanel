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
        let kind = Self::map_entity_kind(&ha.entity_id);

        let name = ha
            .attributes
            .friendly_name
            .clone()
            .unwrap_or_else(|| ha.entity_id.clone());

        let is_on = Self::is_on_state(&kind, &ha.state);
        let value = Self::build_value(&kind, &ha.state, &ha);

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

        Ok(())
    }
}

#[async_trait]
impl HomeAssistantClient for HaHttpClient {
    async fn fetch_dashboard_state(&self) -> AppResult<DashboardState> {
        let mut url = self.base_url()?;
        url.set_path("api/states");

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

        let entities = ha_states.into_iter().map(Self::build_entity).collect();

        Ok(DashboardState { entities })
    }

    async fn toggle(&self, entity_id: &EntityId) -> AppResult<()> {
        let id_str = &entity_id.0;

        let domain = id_str
            .split('.')
            .next()
            .ok_or_else(|| AppError::Internal(format!("Invalid entity_id: {id_str}")))?;

        let body = serde_json::json!({ "entity_id": id_str });

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

        self.call_service("script", "turn_on", body).await
    }
}
