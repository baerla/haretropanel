use askama::Template;
use axum::{
    extract::State,
    response::Html,
};
use tracing::info;

use crate::infrastructure::web::AppState;
use crate::shared::error::AppResult;

#[derive(Debug)]
pub struct EntitySettingsViewModel {
    pub id: String,
    pub name: String,
    pub kind_label: String,
    pub is_on: bool,
    pub has_value: bool,
    pub value: String,
    pub can_toggle: bool,
    pub can_run_script: bool,
    pub can_toggle_cover: bool,
    pub is_selected: bool,
    pub page: usize,
}

impl From<(&crate::domain::Entity, bool, usize)> for EntitySettingsViewModel {
    fn from((e, visible, page): (&crate::domain::Entity, bool, usize)) -> Self {
        let has_value = e.value.is_some() && !e.value.as_ref().unwrap().is_empty();
        let value = e.value.clone().unwrap_or_else(|| "N/A".to_string());
        Self {
            id: e.id.0.clone(),
            name: e.name.clone(),
            kind_label: format!("{:?}", e.kind),
            is_on: e.is_on,
            has_value,
            value,
            can_toggle: matches!(e.kind, crate::domain::EntityKind::Light | crate::domain::EntityKind::Switch),
            can_run_script: false,
            can_toggle_cover: matches!(e.kind, crate::domain::EntityKind::Cover),
            is_selected: visible,
            page,
        }
    }
}

#[derive(askama::Template)]
#[template(path = "entities_settings")]
struct EntitiesSettingsTemplate {
    entities: Vec<EntitySettingsViewModel>,
}

pub async fn get_entity_settings(State(state): State<AppState>) -> AppResult<Html<String>> {
    info!("Settings page loaded");

    let dashboard_state = state.dashboard_service.get_dashboard().await?;
    let cfg = state.dashboard_service.config();
    let visible_ids = state.dashboard_service.get_visible_entity_ids().await.unwrap_or_default();
    let pages = state.dashboard_service.get_entity_pages().await.unwrap_or_default();

    let system_entity_ids: std::collections::HashSet<&str> = [
        cfg.solar_entity_id.as_str(),
        cfg.charger_current_entity_id.as_str(),
        cfg.goe_status_entity_id.as_str(),
        cfg.goe_car_connected_entity_id.as_str(),
        cfg.garage_left_status_entity_id.as_str(),
        cfg.garage_left_action_entity_id.as_str(),
        cfg.garage_right_status_entity_id.as_str(),
        cfg.garage_right_action_entity_id.as_str(),
    ]
    .into_iter()
    .collect();

    let visible_id_set: std::collections::HashSet<&str> = visible_ids.iter().map(|e| e.0.as_str()).collect();

    let entities: Vec<EntitySettingsViewModel> = dashboard_state
        .entities
        .iter()
        .filter(|e| !system_entity_ids.contains(e.id.0.as_str()))
        .map(|e| {
            let is_visible = visible_id_set.contains(e.id.0.as_str());
            let page = pages.get(&e.id.0).copied().unwrap_or(1);
            EntitySettingsViewModel::from((e, is_visible, page))
        })
        .collect();

    let template = EntitiesSettingsTemplate { entities };

    Ok(Html(template.render()?))
}
