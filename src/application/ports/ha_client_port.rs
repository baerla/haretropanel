use async_trait::async_trait;

use crate::{
    domain::{DashboardState, EntityId},
    shared::error::AppResult,
};

#[async_trait]
pub trait HomeAssistantClient: Send + Sync {
    async fn fetch_dashboard_state(&self) -> AppResult<DashboardState>;
    async fn toggle(&self, entity_id: &EntityId) -> AppResult<()>;
    async fn run_script(&self, entity_id: &EntityId) -> AppResult<()>;
    async fn call_service_raw(
        &self,
        domain: &str,
        service: &str,
        body: serde_json::Value,
    ) -> AppResult<()>;
}
