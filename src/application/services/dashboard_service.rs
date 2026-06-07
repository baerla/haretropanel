use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
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

#[derive(Clone, Debug)]
struct SolarSample {
    timestamp: SystemTime,
    watts: f64,
}

#[derive(Clone, Debug)]
struct GoeEnergyTracker {
    last_value: Option<f64>,
    last_change: Option<Instant>,
}

pub struct DashboardService {
    ha_client: Arc<dyn HomeAssistantClient>,
    layout_repo: Arc<dyn DashboardLayoutRepository>,
    cache_config: DashboardCacheConfig,
    state_cache: RwLock<Option<CachedDashboardState>>,
    solar_history: RwLock<VecDeque<SolarSample>>,
    goe_energy_tracker: RwLock<GoeEnergyTracker>,
    config: crate::config::AppConfig,
}

impl DashboardService {
    pub fn new(
        ha_client: Arc<dyn HomeAssistantClient>,
        layout_repo: Arc<dyn DashboardLayoutRepository>,
        cache_config: DashboardCacheConfig,
        config: crate::config::AppConfig,
    ) -> Self {
        Self {
            ha_client,
            layout_repo,
            cache_config,
            state_cache: RwLock::new(None),
            solar_history: RwLock::new(VecDeque::new()),
            goe_energy_tracker: RwLock::new(GoeEnergyTracker {
                last_value: None,
                last_change: None,
            }),
            config,
        }
    }

    fn ttl_secs_for_kind(&self, kind: &EntityKind) -> u64 {
        match kind {
            EntityKind::Light => self
                .cache_config
                .light_ttl_secs
                .unwrap_or(self.cache_config.default_ttl_secs),
            EntityKind::Switch => self
                .cache_config
                .switch_ttl_secs
                .unwrap_or(self.cache_config.default_ttl_secs),
            EntityKind::Sensor => self
                .cache_config
                .sensor_ttl_secs
                .unwrap_or(self.cache_config.default_ttl_secs),
            EntityKind::Climate | EntityKind::Script | EntityKind::Cover => self
                .cache_config
                .climate_ttl_secs
                .unwrap_or(self.cache_config.default_ttl_secs),
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
                    tracing::debug!(
                        entity_count = cached.state.entities.len(),
                        "Returning cached dashboard state (TTL {:?} active)",
                        ttl.saturating_sub(now.duration_since(cached.fetched_at))
                    );
                    return Ok(cached.state.clone());
                }
            }
        }

        tracing::info!("Fetching fresh dashboard state from Home Assistant");
        let fresh = self.ha_client.fetch_dashboard_state().await?;
        let ttl = self.ttl_for_state(&fresh);

        if let Some(solar) = fresh
            .entities
            .iter()
            .find(|e| e.id.0 == self.config.solar_entity_id)
        {
            if let Some(value) = &solar.value {
                if let Ok(watts) = value
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<f64>()
                {
                    let mut history = self.solar_history.write().await;
                    history.push_back(SolarSample { timestamp: SystemTime::now(), watts });

                    let max_age = Duration::from_secs(self.config.solar_history_minutes * 60);
                    while let Some(front) = history.front() {
                        if SystemTime::now()
                            .duration_since(front.timestamp)
                            .unwrap_or(Duration::ZERO)
                            > max_age
                        {
                            history.pop_front();
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        if let Some(goe_energy) = fresh
            .entities
            .iter()
            .find(|e| e.id.0 == self.config.goe_energy_entity_id)
        {
            if let Some(value) = &goe_energy.value {
                let parsed = value
                    .split_whitespace()
                    .next()
                    .map(|raw| raw.replace(',', "."))
                    .and_then(|n| n.parse::<f64>().ok());

                if let Some(kwh) = parsed {
                    let mut tracker = self.goe_energy_tracker.write().await;
                    let delta = tracker
                        .last_value
                        .map(|prev| (kwh - prev).abs())
                        .unwrap_or(self.config.goe_energy_delta_kwh + 1.0);

                    if delta >= self.config.goe_energy_delta_kwh || tracker.last_change.is_none() {
                        tracker.last_change = Some(now);
                    }
                    tracker.last_value = Some(kwh);
                }
            }
        }

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

    pub fn config(&self) -> &crate::config::AppConfig {
        &self.config
    }

    /// Returns the human-readable time when the dashboard was last fetched from HA.
    /// Returns "never" if no fetch has happened yet.
    pub fn last_fetched_label(&self) -> String {
        let cache = self.state_cache.try_read().ok();
        match cache.as_ref().and_then(|c| c.as_ref()) {
            Some(cached) => {
                let now = Instant::now();
                let elapsed = now.duration_since(cached.fetched_at);
                let secs = elapsed.as_secs();
                if secs < 2 {
                    "just now".to_string()
                } else if secs < 60 {
                    format!("{}s ago", secs)
                } else if secs < 3600 {
                    format!("{}m {}s ago", secs / 60, secs % 60)
                } else {
                    format!("{}h {}m ago", secs / 3600, (secs % 3600) / 60)
                }
            }
            None => "never".to_string(),
        }
    }

    pub async fn solar_history_points(&self) -> Vec<(SystemTime, f64)> {
        let history = self.solar_history.read().await;
        history
            .iter()
            .map(|sample| (sample.timestamp, sample.watts))
            .collect()
    }

    pub async fn is_goe_energy_stable(&self) -> bool {
        let tracker = self.goe_energy_tracker.read().await;
        let Some(last_change) = tracker.last_change else {
            return false;
        };
        Instant::now().duration_since(last_change)
            >= Duration::from_secs(self.config.goe_energy_stable_secs)
    }

    pub async fn get_all_entities(&self) -> AppResult<DashboardState> {
        tracing::debug!("get_all_entities: fetching fresh entities");
        self.fetch_state_with_cache().await
    }

    pub async fn get_dashboard(&self) -> AppResult<DashboardState> {
        let all = self.fetch_state_with_cache().await?;
        let visible_ids = self.layout_repo.load_visible_entities().await?;

        if visible_ids.is_empty() {
            return Ok(all);
        }

        let visible_set: HashSet<EntityId> = visible_ids.into_iter().collect();

        let cfg = &self.config;
        // System entities must always be visible even if not in the visible list
        let system_ids: HashSet<String> = [
            cfg.solar_entity_id.clone(),
            cfg.charger_current_entity_id.clone(),
            cfg.goe_status_entity_id.clone(),
            cfg.goe_car_connected_entity_id.clone(),
            cfg.garage_left_entity_id.clone(),
            cfg.garage_right_entity_id.clone(),
        ]
        .into_iter()
        .collect();

        let filtered = DashboardState {
            entities: all
                .entities
                .into_iter()
                .filter(|e| visible_set.contains(&e.id) || system_ids.contains(&e.id.0))
                .collect(),
        };

        Ok(filtered)
    }

    pub async fn get_dashboard_with_refresh(
        &self,
        force_refresh: bool,
    ) -> AppResult<DashboardState> {
        if force_refresh {
            tracing::info!("Dashboard cache invalidated (force refresh)");
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
        tracing::info!(entity_id = %id, "Toggling entity via Home Assistant");
        self.ha_client.toggle(id).await?;
        tracing::debug!(entity_id = %id, "Entity toggled successfully, invalidating cache");
        self.invalidate_cache().await;
        Ok(())
    }

    pub async fn run_script(&self, id: &EntityId) -> AppResult<()> {
        tracing::info!(entity_id = %id, "Running script via Home Assistant");
        self.ha_client.run_script(id).await?;
        tracing::debug!(entity_id = %id, "Script executed successfully, invalidating cache");
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
        application::ports::HomeAssistantClient, config::AppConfig, domain::{DashboardState, Entity, EntityId, EntityKind}, shared::error::AppResult
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
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None), AppConfig::from_env().unwrap());

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
        let service = DashboardService::new(ha, repo, cache_config(60, None), AppConfig::from_env().unwrap());

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
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None), AppConfig::from_env().unwrap());

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
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None), AppConfig::from_env().unwrap());

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
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, Some(1)), AppConfig::from_env().unwrap());

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
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None), AppConfig::from_env().unwrap());

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
        let service = DashboardService::new(ha.clone(), repo, cache_config(60, None), AppConfig::from_env().unwrap());

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
