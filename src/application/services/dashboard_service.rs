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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use tokio::sync::RwLock;

    use super::{DashboardCacheConfig, DashboardLayoutRepository, DashboardService};
    use crate::{
        application::ports::HomeAssistantClient,
        domain::{DashboardState, Entity, EntityId, EntityKind},
        shared::error::AppResult,
    };

    struct FakeHaClient {
        state: RwLock<DashboardState>,
        fetch_count: AtomicUsize,
        toggled: RwLock<Vec<String>>,
        scripts: RwLock<Vec<String>>,
    }

    impl FakeHaClient {
        fn new(state: DashboardState) -> Self {
            Self {
                state: RwLock::new(state),
                fetch_count: AtomicUsize::new(0),
                toggled: RwLock::new(Vec::new()),
                scripts: RwLock::new(Vec::new()),
            }
        }

        async fn set_state(&self, state: DashboardState) {
            *self.state.write().await = state;
        }

        fn fetch_count(&self) -> usize {
            self.fetch_count.load(Ordering::SeqCst)
        }

        async fn toggle_calls(&self) -> Vec<String> {
            self.toggled.read().await.clone()
        }

        async fn script_calls(&self) -> Vec<String> {
            self.scripts.read().await.clone()
        }
    }

    #[async_trait]
    impl HomeAssistantClient for FakeHaClient {
        async fn fetch_dashboard_state(&self) -> AppResult<DashboardState> {
            self.fetch_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.state.read().await.clone())
        }

        async fn toggle(&self, id: &EntityId) -> AppResult<()> {
            self.toggled.write().await.push(id.to_string());
            Ok(())
        }

        async fn run_script(&self, id: &EntityId) -> AppResult<()> {
            self.scripts.write().await.push(id.to_string());
            Ok(())
        }
    }

    struct FakeLayoutRepo {
        visible: RwLock<Vec<EntityId>>,
        pages: RwLock<HashMap<String, usize>>,
    }

    impl FakeLayoutRepo {
        fn new(visible: Vec<EntityId>, pages: HashMap<String, usize>) -> Self {
            Self {
                visible: RwLock::new(visible),
                pages: RwLock::new(pages),
            }
        }
    }

    #[async_trait]
    impl DashboardLayoutRepository for FakeLayoutRepo {
        async fn load_visible_entities(&self) -> AppResult<Vec<EntityId>> {
            Ok(self.visible.read().await.clone())
        }

        async fn save_visible_entities(&self, ids: Vec<EntityId>) -> AppResult<()> {
            *self.visible.write().await = ids;
            Ok(())
        }

        async fn load_entity_pages(&self) -> AppResult<HashMap<String, usize>> {
            Ok(self.pages.read().await.clone())
        }

        async fn save_entity_pages(&self, map: HashMap<String, usize>) -> AppResult<()> {
            *self.pages.write().await = map;
            Ok(())
        }
    }

    fn make_entity(id: &str, kind: EntityKind) -> Entity {
        Entity {
            id: EntityId(id.to_string()),
            name: id.to_string(),
            kind,
            is_on: true,
            value: None,
        }
    }

    fn cache_config(default_ttl_secs: u64, light_ttl_secs: Option<u64>) -> DashboardCacheConfig {
        DashboardCacheConfig {
            default_ttl_secs,
            light_ttl_secs,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        }
    }

    #[tokio::test]
    async fn get_dashboard_returns_all_when_no_visible_ids() {
        let state = DashboardState {
            entities: vec![
                make_entity("light.kitchen", EntityKind::Light),
                make_entity("switch.fan", EntityKind::Switch),
            ],
        };
        let ha = Arc::new(FakeHaClient::new(state));
        let repo = Arc::new(FakeLayoutRepo::new(Vec::new(), HashMap::new()));
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None));

        let dashboard = service.get_dashboard().await.unwrap();

        assert_eq!(dashboard.entities.len(), 2);
        assert_eq!(ha.fetch_count(), 1);
    }

    #[tokio::test]
    async fn get_dashboard_filters_visible_entities() {
        let state = DashboardState {
            entities: vec![
                make_entity("light.kitchen", EntityKind::Light),
                make_entity("switch.fan", EntityKind::Switch),
            ],
        };
        let ha = Arc::new(FakeHaClient::new(state));
        let repo = Arc::new(FakeLayoutRepo::new(
            vec![EntityId("light.kitchen".to_string())],
            HashMap::new(),
        ));
        let service = DashboardService::new(ha, repo, cache_config(60, None));

        let dashboard = service.get_dashboard().await.unwrap();

        assert_eq!(dashboard.entities.len(), 1);
        assert_eq!(dashboard.entities[0].id, EntityId("light.kitchen".to_string()));
    }

    #[tokio::test]
    async fn get_dashboard_with_refresh_forces_new_fetch() {
        let state = DashboardState {
            entities: vec![make_entity("light.kitchen", EntityKind::Light)],
        };
        let ha = Arc::new(FakeHaClient::new(state));
        let repo = Arc::new(FakeLayoutRepo::new(Vec::new(), HashMap::new()));
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None));

        let _ = service.get_dashboard_with_refresh(false).await.unwrap();
        assert_eq!(ha.fetch_count(), 1);

        ha.set_state(DashboardState {
            entities: vec![make_entity("switch.fan", EntityKind::Switch)],
        })
        .await;

        let refreshed = service.get_dashboard_with_refresh(true).await.unwrap();
        assert_eq!(ha.fetch_count(), 2);
        assert_eq!(
            refreshed.entities[0].id,
            EntityId("switch.fan".to_string())
        );
    }

    #[tokio::test]
    async fn uses_cache_within_ttl() {
        let state = DashboardState {
            entities: vec![make_entity("light.kitchen", EntityKind::Light)],
        };
        let ha = Arc::new(FakeHaClient::new(state));
        let repo = Arc::new(FakeLayoutRepo::new(Vec::new(), HashMap::new()));
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None));

        let _ = service.get_all_entities().await.unwrap();
        let _ = service.get_all_entities().await.unwrap();

        assert_eq!(ha.fetch_count(), 1);
    }

    #[tokio::test]
    async fn cache_expires_using_lowest_ttl() {
        let state = DashboardState {
            entities: vec![make_entity("light.kitchen", EntityKind::Light)],
        };
        let ha = Arc::new(FakeHaClient::new(state));
        let repo = Arc::new(FakeLayoutRepo::new(Vec::new(), HashMap::new()));
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, Some(1)));

        let _ = service.get_all_entities().await.unwrap();

        ha.set_state(DashboardState {
            entities: vec![make_entity("switch.fan", EntityKind::Switch)],
        })
        .await;

        tokio::time::sleep(Duration::from_millis(1100)).await;
        let refreshed = service.get_all_entities().await.unwrap();

        assert_eq!(ha.fetch_count(), 2);
        assert_eq!(
            refreshed.entities[0].id,
            EntityId("switch.fan".to_string())
        );
    }

    #[tokio::test]
    async fn toggle_entity_invalidates_cache() {
        let state = DashboardState {
            entities: vec![make_entity("light.kitchen", EntityKind::Light)],
        };
        let ha = Arc::new(FakeHaClient::new(state));
        let repo = Arc::new(FakeLayoutRepo::new(Vec::new(), HashMap::new()));
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None));

        let _ = service.get_all_entities().await.unwrap();
        ha.set_state(DashboardState {
            entities: vec![make_entity("switch.fan", EntityKind::Switch)],
        })
        .await;

        service
            .toggle_entity(&EntityId("light.kitchen".to_string()))
            .await
            .unwrap();
        let refreshed = service.get_all_entities().await.unwrap();

        assert_eq!(ha.fetch_count(), 2);
        assert_eq!(
            refreshed.entities[0].id,
            EntityId("switch.fan".to_string())
        );
        assert_eq!(
            ha.toggle_calls().await,
            vec!["light.kitchen".to_string()]
        );
    }

    #[tokio::test]
    async fn run_script_invalidates_cache() {
        let state = DashboardState {
            entities: vec![make_entity("script.nightly", EntityKind::Script)],
        };
        let ha = Arc::new(FakeHaClient::new(state));
        let repo = Arc::new(FakeLayoutRepo::new(Vec::new(), HashMap::new()));
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None));

        let _ = service.get_all_entities().await.unwrap();
        ha.set_state(DashboardState {
            entities: vec![make_entity("script.morning", EntityKind::Script)],
        })
        .await;

        service
            .run_script(&EntityId("script.nightly".to_string()))
            .await
            .unwrap();
        let refreshed = service.get_all_entities().await.unwrap();

        assert_eq!(ha.fetch_count(), 2);
        assert_eq!(
            refreshed.entities[0].id,
            EntityId("script.morning".to_string())
        );
        assert_eq!(
            ha.script_calls().await,
            vec!["script.nightly".to_string()]
        );
    }
}
