use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use chrono::prelude::*;

use async_trait::async_trait;
use chrono::{DateTime, Timelike};
use tokio::sync::RwLock;

use crate::{
    application::ports::HomeAssistantClient,
    domain::{DashboardState, Entity, EntityId, EntityKind},
    infrastructure::web::viewmodels::{BufferTempChartPoints, ChargerState, PumpViewModel, SolarChartPoints},
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
struct BufferTempSample {
    timestamp: SystemTime,
    buffer_top: f64,
    buffer_bottom: f64,
    solar_flow: f64,
    solar_return: f64,
}

#[derive(Clone, Debug)]
struct PumpSample {
    timestamp: SystemTime,
    pump_on: bool,
    is_correct: bool,
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
    buffer_temp_history: RwLock<VecDeque<BufferTempSample>>,
    pump_history: RwLock<VecDeque<PumpSample>>,
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
            buffer_temp_history: RwLock::new(VecDeque::new()),
            pump_history: RwLock::new(VecDeque::new()),
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

        self.record_buffer_temps(&fresh);

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
            Some(_) => {
                let now = Local::now();
                format!("{}", now.format("%d.%m.%Y %H:%M:%S"))
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

    pub fn parse_solar_watts(&self, state: &DashboardState) -> f64 {
        let cfg = self.config();
        state
            .entities
            .iter()
            .find(|e| e.id.0 == cfg.solar_entity_id)
            .and_then(|e| e.value.as_ref())
            .and_then(|v| {
                v.split_whitespace()
                    .next()
                    .and_then(|n| n.parse::<f64>().ok())
            })
            .unwrap_or(0.0)
    }

    pub async fn compute_solar_chart(&self) -> SolarChartPoints {
        let history = self.solar_history_points().await;
        let cfg = self.config();
        let history_minutes = cfg.solar_history_minutes.max(12);

        let mut labels: Vec<String> = Vec::new();
        let mut values: Vec<f64> = Vec::new();

        for (ts, watts) in history.iter() {
            if let Ok(elapsed) = SystemTime::now().duration_since(*ts) {
                let age_mins = elapsed.as_secs() / 60;
                if age_mins > history_minutes * 60 {
                    continue;
                }
                if let Some(utc_dt) = DateTime::from_timestamp(
                    ts.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    0,
                ) {
                    let local = utc_dt.with_timezone(&Local);
                    let hour = local.hour();
                    if (hour >= 21 || hour < 6) && *watts <= 0.0 {
                        continue;
                    }
                    labels.push(format!("{:02}:{:02}", hour, local.minute()));
                } else {
                    labels.push(format!("-{}m", age_mins));
                }
                values.push(*watts);
            }
        }

        SolarChartPoints { labels, values }
    }

    pub async fn compute_car_state(&self, state: &DashboardState) -> ChargerState {
        let cfg = self.config();

        let charger_entity = state.entities.iter().find(|e| e.id.0 == cfg.charger_current_entity_id);
        let goe_status_entity = state.entities.iter().find(|e| e.id.0 == cfg.goe_status_entity_id);
        let car_connected_entity = state.entities.iter().find(|e| e.id.0 == cfg.goe_car_connected_entity_id);
        let goe_charging_entity = state.entities.iter().find(|e| e.id.0 == cfg.goe_charging_entity_id);

        let charger_value = charger_entity
            .as_ref()
            .and_then(|e| e.value.as_ref())
            .map(|v| v.to_string())
            .unwrap_or_else(|| "0.0 A".to_string());

        let goe_status = goe_status_entity
            .as_ref()
            .and_then(|e| e.value.as_ref())
            .map(|v| v.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let paused = goe_status.contains("Ladestop");

        let car_connected = car_connected_entity
            .as_ref()
            .and_then(|e| e.value.as_ref())
            .map(|v| v == "true" || v == "True")
            .unwrap_or(false);

        let car_charging = goe_charging_entity
            .as_ref()
            .and_then(|e| e.value.as_ref())
            .map(|v| v == "true" || v == "True" || v == "on")
            .unwrap_or(false);

        let goe_status_lower = goe_status.to_lowercase();
        let status_indicates_absent = goe_status_lower.contains("nicht verbunden")
            || goe_status_lower.contains("disconnected")
            || goe_status_lower.contains("deactivated");
        let status_indicates_finished = goe_status_lower.contains("fertig")
            || goe_status_lower.contains("abgeschlossen")
            || goe_status_lower.contains("voll")
            || goe_status_lower.contains("finished");
        let energy_stable = self.is_goe_energy_stable().await;
        let car_present = car_connected || !status_indicates_absent;

        let car_state_label = if !car_present
            || status_indicates_absent
            || (status_indicates_finished && energy_stable)
        {
            if status_indicates_absent || (!car_present && !status_indicates_finished) {
                "Nicht angeschlossen".to_string()
            } else {
                "Auto voll".to_string()
            }
        } else if car_connected {
            "Angeschlossen".to_string()
        } else {
            "Nicht angeschlossen".to_string()
        };

        let car_state_class = if car_state_label == "Auto voll" {
            "is-full".to_string()
        } else if car_state_label == "Angeschlossen" {
            "is-charging".to_string()
        } else {
            "is-empty".to_string()
        };

        ChargerState {
            charger_value,
            status: goe_status,
            paused,
            car_connected,
            car_charging,
            car_state_label,
            car_state_class,
        }
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
            cfg.goe_charging_entity_id.clone(),
            cfg.garage_left_status_entity_id.clone(),
            cfg.garage_right_status_entity_id.clone(),
            cfg.solar_buffer_bottom_entity_id.clone(),
            cfg.solar_buffer_top_entity_id.clone(),
            cfg.solar_flow_entity_id.clone(),
            cfg.solar_return_entity_id.clone(),
            cfg.solar_pump_entity_id.clone(),
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

    fn record_buffer_temps(&self, state: &DashboardState) {
        let cfg = self.config();
        let find = |id: &str| state.entities.iter().find(|e| e.id.0 == id);

        let buffer_top = find(&cfg.solar_buffer_top_entity_id).and_then(|e| e.value.as_deref());
        let buffer_bottom = find(&cfg.solar_buffer_bottom_entity_id).and_then(|e| e.value.as_deref());
        let solar_flow = find(&cfg.solar_flow_entity_id).and_then(|e| e.value.as_deref());
        let solar_return = find(&cfg.solar_return_entity_id).and_then(|e| e.value.as_deref());

        let parse = |v: Option<&str>| {
            v.and_then(|s| s.split_whitespace().next())
                .and_then(|n| n.parse::<f64>().ok())
                .unwrap_or(0.0)
        };

        let sample = BufferTempSample {
            timestamp: SystemTime::now(),
            buffer_top: parse(buffer_top),
            buffer_bottom: parse(buffer_bottom),
            solar_flow: parse(solar_flow),
            solar_return: parse(solar_return),
        };

        let max_age = Duration::from_secs(self.config.solar_history_minutes * 60);
        if let Ok(mut history) = self.buffer_temp_history.try_write() {
            history.push_back(sample);
            while let Some(el) = history.iter().next() {
                if SystemTime::now()
                    .duration_since(el.timestamp)
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

    pub async fn compute_buffer_temp_chart(&self) -> BufferTempChartPoints {
            let history = self.buffer_temp_history.try_read();
            match history {
                Ok(h) => {
                    if h.is_empty() {
                        return BufferTempChartPoints {
                            labels: vec![],
                            buffer_top: vec![],
                            buffer_bottom: vec![],
                            solar_flow: vec![],
                            solar_return: vec![],
                        };
                    }
                    let mut labels = Vec::with_capacity(h.len());
                    let mut buffer_top = Vec::with_capacity(h.len());
                    let mut buffer_bottom = Vec::with_capacity(h.len());
                    let mut solar_flow = Vec::with_capacity(h.len());
                    let mut solar_return = Vec::with_capacity(h.len());

                    for sample in h.iter() {
                        if let Ok(elapsed) = SystemTime::now().duration_since(sample.timestamp) {
                            let age_mins = elapsed.as_secs() / 60;
                            if age_mins > 60 {
                                continue;
                            }
                            if let Some(utc_dt) = DateTime::from_timestamp(
                                sample
                                    .timestamp
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs() as i64,
                                0,
                            ) {
                                let local = utc_dt.with_timezone(&Local);
                                labels.push(format!("{:02}:{:02}", local.hour(), local.minute()));
                            }
                            buffer_top.push(sample.buffer_top);
                            buffer_bottom.push(sample.buffer_bottom);
                            solar_flow.push(sample.solar_flow);
                            solar_return.push(sample.solar_return);
                        }
                    }
                    BufferTempChartPoints {
                        labels,
                        buffer_top,
                        buffer_bottom,
                        solar_flow,
                        solar_return,
                    }
                }
                Err(_) => BufferTempChartPoints {
                    labels: vec![],
                    buffer_top: vec![],
                    buffer_bottom: vec![],
                    solar_flow: vec![],
                    solar_return: vec![],
                },
            }
    }

    pub fn compute_pump_status(&self, state: &DashboardState) -> PumpViewModel {
        let cfg = self.config();

        let find = |id: &str| state.entities.iter().find(|e| e.id.0 == id);

        let pump_entity = find(cfg.solar_pump_entity_id.as_str());
        let pump_on = pump_entity.map(|e| e.is_on).unwrap_or(false);

        let parse = |e: Option<&Entity>| {
            e.and_then(|x| x.value.as_ref())
                .and_then(|v| v.split_whitespace().next())
                .and_then(|n| n.parse::<f64>().ok())
                .unwrap_or(0.0)
        };

        let buffer_bottom_temp = parse(find(&cfg.solar_buffer_bottom_entity_id));
        let solar_return_temp = parse(find(&cfg.solar_return_entity_id));

        let is_correct = if pump_on {
            solar_return_temp > buffer_bottom_temp
        } else {
            solar_return_temp <= buffer_bottom_temp
        };

        let sample = PumpSample {
            timestamp: SystemTime::now(),
            pump_on,
            is_correct,
        };
        tokio::task::block_in_place(|| {
            if let Ok(mut history) = self.pump_history.try_write() {
                history.push_back(sample);
                while history.len() > 60 {
                    history.pop_front();
                }
            }
        });

        PumpViewModel {
            pump_on,
            is_correct,
            status_label: if pump_on { "ON".to_string() } else { "OFF".to_string() },
            css_class: if is_correct {
                if pump_on {
                    "pump-on".to_string()
                } else {
                    "pump-off".to_string()
                }
            } else {
                "pump-wrong".to_string()
            },
        }
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
