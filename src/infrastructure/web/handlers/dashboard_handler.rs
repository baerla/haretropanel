use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
    Form,
};
use chrono::{DateTime, Timelike};
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

    let charger_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.charger_current_entity_id);
    let goe_status_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.goe_status_entity_id);
    let goe_car_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.goe_car_connected_entity_id);
    let goe_charging_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.goe_charging_entity_id);
    let garage_left_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.garage_left_entity_id);
    let garage_right_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.garage_right_entity_id);

    let solar_watts = solar_entity
        .and_then(|e| e.value.clone())
        .and_then(|v| {
            v.split_whitespace()
                .next()
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(0.0);

    let max_watts_label = if cfg.solar_max_watts > 0.0 {
        format!("{:.0} W max", cfg.solar_max_watts)
    } else {
        "Max unavailable".to_string()
    };

    let history = state.dashboard_service.solar_history_points().await;
    let mut labels: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();
    // Show last 12 hours, skip nighttime (0 values)
    let history_minutes = cfg.solar_history_minutes.max(12);

    for (ts, watts) in history.iter() {
        if let Ok(elapsed) = std::time::SystemTime::now().duration_since(*ts) {
            let age_mins = elapsed.as_secs() / 60;
            if age_mins > history_minutes * 60 {
                continue;
            }
            if let Some(local) = DateTime::from_timestamp(
                ts.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                0,
            ) {
                let hour = local.hour();
                // Skip night hours (21:00 - 06:00)
                if (hour >= 21 || hour < 6) && *watts <= 0.0 {
                    continue;
                }
                labels.push(format!("{:02}:{:02}", hour, local.minute()));
            } else {
                labels.push(format!("-{}m", age_mins));
            }
            values.push(format!("{:.0}", watts));
        }
    }

    // Current solar_watts is already included above if non-zero; no separate append needed
    let solar_vm = SolarViewModel {
        watts_label: format!("{:.0} W", solar_watts),
        max_watts_label,
        chart_labels_js: format!(
            "[{}]",
            labels
                .iter()
                .map(|l| format!("\"{}\"", l))
                .collect::<Vec<_>>()
                .join(",")
        ),
        chart_values_js: format!("[{}]", values.join(",")),
    };

    let charger_value = charger_entity
        .and_then(|e| e.value.clone())
        .unwrap_or_else(|| "0 W".to_string());

    let goe_status = goe_status_entity
        .and_then(|e| e.value.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    tracing::debug!(
        charger_entity_id = cfg.charger_current_entity_id,
        charger_value,
        goe_status,
        "Charger entity values extracted"
    );

    let mut car_connected = false;
    let mut car_charging = false;
    if let Some(e) = goe_car_entity {
        car_connected = e.is_on || e.value.as_deref() == Some("on");
    }
    if let Some(e) = goe_charging_entity {
        car_charging = e.is_on || e.value.as_deref() == Some("on");
    }

    let goe_status_lower = goe_status.to_lowercase();
    let status_indicates_absent = goe_status_lower.contains("nicht")
        || goe_status_lower.contains("stopp")
        || goe_status_lower.contains("stop")
        || goe_status_lower.contains("offline")
        || goe_status_lower.contains("keine");
    let status_indicates_finished = goe_status_lower.contains("fertig")
        || goe_status_lower.contains("abgeschlossen")
        || goe_status_lower.contains("voll")
        || goe_status_lower.contains("finished");
    let energy_stable = state.dashboard_service.is_goe_energy_stable().await;
    let car_present = car_connected || !status_indicates_absent;

    let car_state_label = if !car_present
        || status_indicates_absent
        || (status_indicates_finished && energy_stable)
    {
        if status_indicates_absent || (!car_present && !status_indicates_finished) {
            "Nicht angeschlossen"
        } else {
            "Auto voll"
        }
    } else if car_connected {
        "Angeschlossen"
    } else {
        "Nicht angeschlossen"
    };
    let car_state_class = if car_state_label == "Auto voll" {
        "is-full"
    } else if car_state_label == "Am Laden" {
        "is-charging"
    } else {
        "is-empty"
    };

    let charger_vm = ChargerViewModel {
        amps_label: charger_value,
        status_label: goe_status.clone(),
        car_state_label: car_state_label.to_string(),
        car_state_class: car_state_class.to_string(),
        car_connected,
        car_charging,
        pill_paused: goe_status.contains("Ladestop"),
    };

    let make_garage_vm = |entity: Option<&crate::domain::Entity>, default_name: &str| {
        let id = entity
            .map(|e| e.id.0.clone())
            .unwrap_or_else(|| default_name.to_string());
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
            id,
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
        cfg.garage_left_entity_id.as_str(),
        cfg.garage_right_entity_id.as_str(),
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
        garage_left: make_garage_vm(garage_left_entity, "Garage Left"),
        garage_right: make_garage_vm(garage_right_entity, "Garage Right"),
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
