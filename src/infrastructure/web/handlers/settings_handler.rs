use std::collections::{HashMap, HashSet};

use askama::Template;
use axum::{
    extract::{RawForm, State},
    response::{Html, IntoResponse, Redirect},
};
use tracing::info;

use crate::{
    domain::EntityId,
    infrastructure::web::viewmodels::{
        EntitiesSettingsPageViewModel, EntitiesSettingsTemplate, EntitySettingsViewModel,
    },
    infrastructure::web::AppState,
    shared::error::AppResult,
};

pub async fn get_entity_settings(
    State(state): State<AppState>,
) -> AppResult<impl IntoResponse> {
    let all_state = state.dashboard_service.get_all_entities().await?;

    let selected_ids = state.dashboard_service.get_visible_entity_ids().await?;
    let selected_set: HashSet<String> = selected_ids.into_iter().map(|id| id.0).collect();

    let page_map = state.dashboard_service.get_entity_pages().await?;

    let entities = all_state
        .entities
        .into_iter()
        .map(|e| {
            let id = e.id.to_string();
            let is_selected = selected_set.contains(&id);
            let page = page_map.get(&id).cloned().unwrap_or(1);

            EntitySettingsViewModel {
                id,
                name: e.name,
                is_selected,
                page,
            }
        })
        .collect();

    let vm = EntitiesSettingsPageViewModel { entities };

    let template = EntitiesSettingsTemplate {
        entities: &vm.entities,
    };

    let rendered = template.render()?;
    Ok(Html(rendered))
}

pub async fn post_entity_settings(
    State(state): State<AppState>,
    RawForm(body): RawForm,
) -> AppResult<impl IntoResponse> {
    let parsed = url::form_urlencoded::parse(&body);

    let mut visible_ids: Vec<EntityId> = Vec::new();
    let mut page_assignments: HashMap<String, usize> = HashMap::new();

    for (key_cow, value_cow) in parsed {
        let key = key_cow.as_ref();
        let value = value_cow.as_ref();

        if key == "visible" {
            visible_ids.push(EntityId(value.to_string()));
            continue;
        }

        if let Some(entity_id) = key.strip_prefix("page_") {
            if let Ok(page) = value.parse::<usize>() {
                if page >= 1 {
                    page_assignments.insert(entity_id.to_string(), page);
                }
            }
        }
    }

    info!("Saving {} visible entities", visible_ids.len());
    info!("Saving {} page assignments", page_assignments.len());

    state
        .dashboard_service
        .save_visible_entities(visible_ids)
        .await?;

    state
        .dashboard_service
        .save_entity_pages(page_assignments)
        .await?;

    Ok(Redirect::to("/"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::body::Bytes;
    use axum::extract::State;
    use axum::extract::{RawForm};
    use tokio::sync::RwLock;

    use super::post_entity_settings;
    use crate::application::services::{DashboardLayoutRepository, DashboardService};
    use crate::application::services::dashboard_service::DashboardCacheConfig;
    use crate::application::ports::HomeAssistantClient;
    use crate::domain::{DashboardState, EntityId};
    use crate::infrastructure::web::AppState;
    use crate::shared::error::AppResult;

    struct NullHaClient;

    #[async_trait]
    impl HomeAssistantClient for NullHaClient {
        async fn fetch_dashboard_state(&self) -> AppResult<DashboardState> {
            Ok(DashboardState { entities: vec![] })
        }

        async fn toggle(&self, _id: &EntityId) -> AppResult<()> {
            Ok(())
        }

        async fn run_script(&self, _id: &EntityId) -> AppResult<()> {
            Ok(())
        }
    }

    struct RecordingLayoutRepo {
        visible: RwLock<Vec<EntityId>>,
        pages: RwLock<HashMap<String, usize>>,
    }

    impl RecordingLayoutRepo {
        fn new() -> Self {
            Self {
                visible: RwLock::new(Vec::new()),
                pages: RwLock::new(HashMap::new()),
            }
        }

        async fn visible_ids(&self) -> Vec<String> {
            self.visible
                .read()
                .await
                .iter()
                .map(|id| id.0.clone())
                .collect()
        }

        async fn page_assignments(&self) -> HashMap<String, usize> {
            self.pages.read().await.clone()
        }
    }

    #[async_trait]
    impl DashboardLayoutRepository for RecordingLayoutRepo {
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

    fn cache_config() -> DashboardCacheConfig {
        DashboardCacheConfig {
            default_ttl_secs: 5,
            light_ttl_secs: None,
            switch_ttl_secs: None,
            sensor_ttl_secs: None,
            climate_ttl_secs: None,
        }
    }

    #[tokio::test]
    async fn post_entity_settings_parses_visible_and_pages() {
        let repo = Arc::new(RecordingLayoutRepo::new());
        let service = Arc::new(DashboardService::new(
            Arc::new(NullHaClient),
            repo.clone(),
            cache_config(),
        ));
        let state = AppState {
            dashboard_service: service,
        };

        let body = "visible=light.kitchen&visible=switch.fan&page_light.kitchen=2&page_switch.fan=0&page_sensor.temp=3";
        let response = post_entity_settings(
            State(state),
            RawForm(Bytes::from(body.to_string())),
        )
        .await;

        assert!(response.is_ok());
        assert_eq!(
            repo.visible_ids().await,
            vec!["light.kitchen".to_string(), "switch.fan".to_string()]
        );

        let pages = repo.page_assignments().await;
        assert_eq!(pages.get("light.kitchen"), Some(&2));
        assert_eq!(pages.get("sensor.temp"), Some(&3));
        assert!(!pages.contains_key("switch.fan"));
    }
}
