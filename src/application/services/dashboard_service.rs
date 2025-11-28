use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::{
    application::ports::HomeAssistantClient,
    domain::{DashboardState, EntityId, EntityKind},
    shared::error::AppResult,
};

#[async_trait]
pub trait DashboardLayoutRepository: Send + Sync {
    async fn load_visible_entities(&self) -> AppResult<Vec<EntityId>>;
    async fn save_visible_entities(&self, ids: Vec<EntityId>) -> AppResult<()>;

    async fn load_entity_pages(&self) -> AppResult<HashMap<String, usize>>;
    async fn save_entity_pages(&self, map: HashMap<String, usize>) -> AppResult<()>;
}

#[derive(Clone, Debug)]
pub struct DashboardCacheConfig {
    pub default_ttl_secs: u64,
    pub light_ttl_secs: Option<u64>,
    pub switch_ttl_secs: Option<u64>,
    pub sensor_ttl_secs: Option<u64>,
    pub climate_ttl_secs: Option<u64>,
}

#[derive(Clone)]
struct CachedDashboardState {
    state: DashboardState,
    fetched_at: Instant,
}

pub struct DashboardService {
    ha_client: Arc<dyn HomeAssistantClient>,
    layout_repo: Arc<dyn DashboardLayoutRepository>,
    cache_config: DashboardCacheConfig,
    state_cache: RwLock<Option<CachedDashboardState>>,
}

impl DashboardService {
    pub fn new(
        ha_client: Arc<dyn HomeAssistantClient>,
        layout_repo: Arc<dyn DashboardLayoutRepository>,
        cache_config: DashboardCacheConfig,
    ) -> Self {
        Self {
            ha_client,
            layout_repo,
            cache_config,
            state_cache: RwLock::new(None),
        }
    }

    fn ttl_secs_for_kind(&self, kind: &EntityKind) -> u64 {
        match kind {
            EntityKind::Light => self.cache_config.light_ttl_secs.unwrap_or(self.cache_config.default_ttl_secs),
            EntityKind::Switch => self.cache_config.switch_ttl_secs.unwrap_or(self.cache_config.default_ttl_secs),
            EntityKind::Sensor => self.cache_config.sensor_ttl_secs.unwrap_or(self.cache_config.default_ttl_secs),
            EntityKind::Climate | EntityKind::Script => {
                self.cache_config.climate_ttl_secs.unwrap_or(self.cache_config.default_ttl_secs)
            }
        }
    }

    fn ttl_for_state(&self, state: &DashboardState) -> Duration {
        let ttl_secs = state
            .entities
            .iter()
            .map(|e| self.ttl_secs_for_kind(&e.kind))
            .min()
            .unwrap_or(self.cache_config.default_ttl_secs);

        Duration::from_secs(ttl_secs)
    }

    async fn fetch_state_with_cache(&self) -> AppResult<DashboardState> {
        let now = Instant::now();

        {
            let cache = self.state_cache.read().await;

            if let Some(cached) = &*cache {
                let ttl = self.ttl_for_state(&cached.state);
                if now.duration_since(cached.fetched_at) <= ttl {
                    return Ok(cached.state.clone());
                }
            }
        }

        let fresh = self.ha_client.fetch_dashboard_state().await?;
        let ttl = self.ttl_for_state(&fresh);

        {
            let mut cache = self.state_cache.write().await;
            *cache = Some(CachedDashboardState {
                state: fresh.clone(),
                fetched_at: now,
            });
        }

        tracing::debug!(
            "Refreshed dashboard state from HA (effective TTL = {:?})",
            ttl
        );

        Ok(fresh)
    }

    async fn invalidate_cache(&self) {
        let mut cache = self.state_cache.write().await;
        *cache = None;
    }

    pub async fn get_all_entities(&self) -> AppResult<DashboardState> {
        self.fetch_state_with_cache().await
    }

    pub async fn get_dashboard(&self) -> AppResult<DashboardState> {
        let all = self.fetch_state_with_cache().await?;
        let visible_ids = self.layout_repo.load_visible_entities().await?;

        if visible_ids.is_empty() {
            return Ok(all);
        }

        let visible_set: HashSet<_> = visible_ids.into_iter().collect();

        let filtered = DashboardState {
            entities: all
                .entities
                .into_iter()
                .filter(|e| visible_set.contains(&e.id))
                .collect(),
        };

        Ok(filtered)
    }

    pub async fn get_dashboard_with_refresh(
        &self,
        force_refresh: bool,
    ) -> AppResult<DashboardState> {
        if force_refresh {
            self.invalidate_cache().await;
        }
        self.get_dashboard().await
    }

    pub async fn get_visible_entity_ids(&self) -> AppResult<Vec<EntityId>> {
        self.layout_repo.load_visible_entities().await
    }

    pub async fn save_visible_entities(&self, ids: Vec<EntityId>) -> AppResult<()> {
        self.layout_repo.save_visible_entities(ids).await
    }

    pub async fn get_entity_pages(&self) -> AppResult<HashMap<String, usize>> {
        self.layout_repo.load_entity_pages().await
    }

    pub async fn save_entity_pages(&self, map: HashMap<String, usize>) -> AppResult<()> {
        self.layout_repo.save_entity_pages(map).await
    }

    pub async fn toggle_entity(&self, id: &EntityId) -> AppResult<()> {
        self.ha_client.toggle(id).await?;
        self.invalidate_cache().await;
        Ok(())
    }

    pub async fn run_script(&self, id: &EntityId) -> AppResult<()> {
        self.ha_client.run_script(id).await?;
        self.invalidate_cache().await;
        Ok(())
    }
}
