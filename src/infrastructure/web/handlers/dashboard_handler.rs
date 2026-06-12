use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
    Form,
};
use serde::Deserialize;
use tracing::info;

use crate::{
    domain::EntityId,
    infrastructure::web::{
        viewmodels::{
            ChargerViewModel, DashboardTemplate, DashboardViewModel, GarageDoorViewModel,
            SolarViewModel,
        },
        AppState,
    },
    shared::error::AppResult,
};

#[derive(Debug, Deserialize)]
pub struct DashboardQuery {
    pub page: Option<usize>,
    pub force_refresh: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToggleForm {
    pub entity_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RunScriptForm {
    pub entity_id: String,
}

pub async fn get_dashboard(
    State(state): State<AppState>,
    Query(query): Query<DashboardQuery>,
) -> AppResult<impl IntoResponse> {
    let _requested_page = query.page.unwrap_or(1);
    let force_refresh = query.force_refresh.is_some();

    tracing::debug!(force_refresh, "Serving dashboard page");
    let dashboard_state = state
        .dashboard_service
        .get_dashboard_with_refresh(force_refresh)
        .await?;

    let cfg = state.dashboard_service.config();

    let last_updated_label = state.dashboard_service.last_fetched_label();

    let solar_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.solar_entity_id);
    tracing::debug!(
        solar_entity_id = cfg.solar_entity_id,
        solar_found = solar_entity.is_some(),
        solar_entity_value = ?solar_entity.and_then(|e| e.value.as_ref()),
        "Finding solar entity"
    );

    let garage_left_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.garage_left_status_entity_id);
    let garage_right_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.garage_right_status_entity_id);

    let solar_watts = state.dashboard_service.parse_solar_watts(&dashboard_state);

    let max_watts_label = if cfg.solar_max_watts > 0.0 {
        format!("{:.0} W max", cfg.solar_max_watts)
    } else {
        "Max unavailable".to_string()
    };

    let chart_points = state.dashboard_service.compute_solar_chart().await;
    let chart_labels: Vec<String> = chart_points.labels;
    let chart_values: Vec<f64> = chart_points.values;

    let solar_vm = SolarViewModel {
        watts_label: format!("{:.0} W", solar_watts),
        max_watts_label,
        chart_labels_js: format!(
            "[{}]",
            chart_labels
                .iter()
                .map(|l| format!("\"{}\"", l))
                .collect::<Vec<_>>()
                .join(",")
        ),
        chart_values_js: format!("[{}]", chart_values.iter().map(|v| format!("{:.0}", v)).collect::<Vec<_>>().join(",")),
    };

    let charger_state = state.dashboard_service.compute_car_state(&dashboard_state).await;

    let charger_vm = ChargerViewModel {
        amps_label: charger_state.charger_value,
        status_label: charger_state.status.clone(),
        car_state_label: charger_state.car_state_label,
        car_state_class: charger_state.car_state_class,
        car_connected: charger_state.car_connected,
        car_charging: charger_state.car_charging,
        pill_paused: charger_state.paused,
    };

    let make_garage_vm = |entity: Option<&crate::domain::Entity>, entity_id: &str, default_name: &str| {
        let name = entity
            .map(|e| e.name.clone())
            .unwrap_or_else(|| default_name.to_string());
        let is_open = entity.map(|e| e.is_on).unwrap_or(false);
        let status_label = if is_open { "Open" } else { "Closed" };
        let action_label = if is_open { "Close" } else { "Open" };
        let button_class = if is_open {
            "garage-btn garage-open"
        } else {
            "garage-btn garage-closed"
        };

        GarageDoorViewModel {
            id: entity_id.to_string(),
            name,
            status_label: status_label.to_string(),
            action_label: action_label.to_string(),
            button_class: button_class.to_string(),
        }
    };

    let requested_page = query.page.unwrap_or(1);
    let entity_pages = state.dashboard_service.get_entity_pages().await.unwrap_or_default();

    // Collect regular entities (exclude system entities from pagination)
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

    let regular_entities: Vec<&crate::domain::Entity> = dashboard_state
        .entities
        .iter()
        .filter(|e| !system_entity_ids.contains(e.id.0.as_str()))
        .collect();

    // Build page tabs from entity pages config
    let mut page_list: Vec<(usize, String)> = Vec::new(); // (page_num, entity_name)
    for entity in &regular_entities {
        if let Some(&page) = entity_pages.get(&entity.id.0) {
            if page > 0 && page_list.iter().all(|&(p, _)| p != page) {
                page_list.push((page, format!("Page {}", page)));
            }
        }
    }
    page_list.sort_by_key(|&(p, _)| p);
    let page_tabs: Vec<String> = page_list.into_iter().map(|(_, name)| name).collect();
    let active_tab = if requested_page > 0 && requested_page <= page_tabs.len() {
        requested_page
    } else {
        1
    };

    // Filter entities for current page
    let page_entities: Vec<crate::infrastructure::web::viewmodels::EntityViewModel> = regular_entities
        .into_iter()
        .filter(|e| {
            entity_pages
                .get(&e.id.0)
                .map(|&p| p == active_tab)
                .unwrap_or(false)
        })
        .map(|e| e.into())
        .collect();

    let dashboard_vm = DashboardViewModel {
        solar: solar_vm,
        charger: charger_vm,
        garage_left: make_garage_vm(garage_left_entity, cfg.garage_left_action_entity_id.as_str(), "Garage Left"),
        garage_right: make_garage_vm(garage_right_entity, cfg.garage_right_action_entity_id.as_str(), "Garage Right"),
        demo_mode: cfg.demo_mode,
        last_updated: last_updated_label,
        page_tabs,
        active_tab,
        page_entities,
    };

    let template = DashboardTemplate {
        dashboard: &dashboard_vm,
    };

    let rendered = template.render()?;
    Ok(Html(rendered))
}

pub async fn post_toggle(
    State(state): State<AppState>,
    Form(form): Form<ToggleForm>,
) -> AppResult<impl IntoResponse> {
    let id = EntityId(form.entity_id);
    info!("Toggling entity via POST /toggle: {}", id);

    state.dashboard_service.toggle_entity(&id).await?;

    Ok(Redirect::to("/"))
}

pub async fn post_run_script(
    State(state): State<AppState>,
    Form(form): Form<RunScriptForm>,
) -> AppResult<impl IntoResponse> {
    let id = EntityId(form.entity_id);
    info!("Running script via POST /run_script: {}", id);

    state.dashboard_service.run_script(&id).await?;

    Ok(Redirect::to("/"))
}

pub async fn get_redirect_to_root() -> impl IntoResponse {
    Redirect::to("/")
}
