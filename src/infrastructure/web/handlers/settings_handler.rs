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

pub async fn get_entity_settings(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    tracing::debug!("Loading entity settings page");
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

    Ok(Redirect::to("/?force_refresh=1"))
}
