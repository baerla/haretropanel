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

    let dashboard_state = state
        .dashboard_service
        .get_dashboard_with_refresh(force_refresh)
        .await?;

    let cfg = state.dashboard_service.config();

    let solar_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.solar_entity_id);
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

    let percent = if cfg.solar_max_watts <= 0.0 {
        0
    } else {
        ((solar_watts / cfg.solar_max_watts) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8
    };

    let max_watts_label = if cfg.solar_max_watts > 0.0 {
        format!("{:.0} W max", cfg.solar_max_watts)
    } else {
        "Max unavailable".to_string()
    };

    let history = state.dashboard_service.solar_history_points().await;
    let now = std::time::Instant::now();
    let mut labels: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();
    let max_points = (cfg.solar_history_minutes * 60 / cfg.solar_sample_secs).max(1) as usize;

    for (idx, (ts, watts)) in history.iter().enumerate() {
        if idx + max_points < history.len() {
            continue;
        }
        let age = now.duration_since(*ts).as_secs();
        let mins = age / 60;
        labels.push(format!("-{}m", mins));
        values.push(format!("{:.0}", watts));
    }

    if labels.is_empty() {
        labels.push("now".to_string());
        values.push(format!("{:.0}", solar_watts));
    }

    let solar_vm = SolarViewModel {
        watts_label: format!("{:.0} W", solar_watts),
        percent,
        max_watts_label,
        chart_labels_js: format!("[{}]", labels.iter().map(|l| format!("\"{}\"", l)).collect::<Vec<_>>().join(",")),
        chart_values_js: format!("[{}]", values.join(",")),
    };

    let charger_value = charger_entity
        .and_then(|e| e.value.clone())
        .unwrap_or_else(|| "0 W".to_string());

    let goe_status = goe_status_entity
        .and_then(|e| e.value.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let mut car_connected = false;
    if let Some(e) = goe_car_entity {
        car_connected = e.is_on || e.value.as_deref() == Some("on");
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

    let car_state_label = if car_present && (energy_stable || status_indicates_finished) {
        "Car full"
    } else {
        "Car not present"
    };
    let car_state_class = if car_state_label == "Car full" {
        "is-full"
    } else {
        "is-empty"
    };

    let charger_vm = ChargerViewModel {
        amps_label: charger_value,
        status_label: goe_status,
        car_state_label: car_state_label.to_string(),
        car_state_class: car_state_class.to_string(),
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

    let dashboard_vm = DashboardViewModel {
        solar: solar_vm,
        charger: charger_vm,
        garage_left: make_garage_vm(garage_left_entity, "Garage Left"),
        garage_right: make_garage_vm(garage_right_entity, "Garage Right"),
        demo_mode: cfg.demo_mode,
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
