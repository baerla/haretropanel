use askama::Template;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse},
};
use serde::Deserialize;

use crate::infrastructure::web::{
    viewhelpers::make_garage_door_vm,
    viewmodels::{
        BufferTempViewModel, ChargerViewModel, DashboardTemplate, DashboardViewModel,
        PumpViewModel, SolarViewModel,
    },
    AppState,
};
use crate::shared::error::AppResult;

#[derive(Debug, Deserialize)]
pub struct DashboardQuery {
    pub page: Option<usize>,
    pub force_refresh: Option<String>,
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

    // Buffer temp chart data
    let buffer_chart_points = state.dashboard_service.compute_buffer_temp_chart().await;
    let buffer_labels: Vec<String> = buffer_chart_points.labels;
    let buffer_top: Vec<f64> = buffer_chart_points.buffer_top;
    let buffer_bottom: Vec<f64> = buffer_chart_points.buffer_bottom;
    let solar_flow: Vec<f64> = buffer_chart_points.solar_flow;
    let solar_return: Vec<f64> = buffer_chart_points.solar_return;

    // Pump status
    let pump_vm = state
        .dashboard_service
        .compute_pump_status(&dashboard_state);

    // Compute pump states for status bar
    let pump_states = state.dashboard_service.compute_pump_status_history().await;
    let pump_states_js = format!(
        "[{}]",
        pump_states
            .iter()
            .map(|(t, on)| {
                let epoch = t
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::ZERO)
                    .as_millis();
                format!("{{\"t\":{},\"on\":{}}}", epoch, on)
            })
            .collect::<Vec<_>>()
            .join(",")
    );

    // Compute current buffer top temperature
    let buffer_top_val = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.solar_buffer_top_entity_id)
        .and_then(|e| e.value.as_deref())
        .and_then(|v| v.split_whitespace().next())
        .and_then(|n| n.parse::<f64>().ok())
        .map(|v| format!("{:.1}°C", v))
        .unwrap_or_else(|| "--°C".to_string());

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
        chart_values_js: format!(
            "[{}]",
            chart_values
                .iter()
                .map(|v| format!("{:.0}", v))
                .collect::<Vec<_>>()
                .join(",")
        ),
        history_minutes: cfg.solar_history_minutes,
    };

    let charger_state = state
        .dashboard_service
        .compute_car_state(&dashboard_state)
        .await;

    let charger_vm = ChargerViewModel {
        amps_label: charger_state.charger_value,
        status_label: charger_state.status.clone(),
        car_state_label: charger_state.car_state_label,
        car_state_class: charger_state.car_state_class,
        car_connected: charger_state.car_connected,
        car_charging: charger_state.car_charging,
        pill_paused: charger_state.paused,
    };

    let requested_page = query.page.unwrap_or(1);
    let entity_pages = state
        .dashboard_service
        .get_entity_pages()
        .await
        .unwrap_or_default();

    // Collect regular entities (exclude system entities from pagination)
    let system_entity_ids = cfg.system_entity_ids();

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
    let page_entities: Vec<crate::infrastructure::web::viewmodels::EntityViewModel> =
        regular_entities
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
        garage_left: make_garage_door_vm(
            garage_left_entity,
            cfg.garage_left_action_entity_id.as_str(),
            "Garage Left",
        ),
        garage_right: make_garage_door_vm(
            garage_right_entity,
            cfg.garage_right_action_entity_id.as_str(),
            "Garage Right",
        ),
        buffer_chart: BufferTempViewModel {
            chart_labels_js: format!(
                "[{}]",
                buffer_labels
                    .iter()
                    .map(|l| format!("\"{}\"", l))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            chart_buffer_top_js: format!(
                "[{}]",
                buffer_top
                    .iter()
                    .map(|v| format!("{:.1}", v))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            chart_buffer_bottom_js: format!(
                "[{}]",
                buffer_bottom
                    .iter()
                    .map(|v| format!("{:.1}", v))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            chart_solar_flow_js: format!(
                "[{}]",
                solar_flow
                    .iter()
                    .map(|v| format!("{:.1}", v))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            chart_solar_return_js: format!(
                "[{}]",
                solar_return
                    .iter()
                    .map(|v| format!("{:.1}", v))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        },
        pump_status: PumpViewModel {
            pump_on: pump_vm.pump_on,
            is_correct: pump_vm.is_correct,
            status_label: pump_vm.status_label,
            css_class: pump_vm.css_class,
        },
        demo_mode: cfg.demo_mode,
        last_updated: last_updated_label,
        page_tabs,
        active_tab,
        page_entities,
        pump_states_js,
        buffer_top_temp: buffer_top_val,
    };

    let template = DashboardTemplate {
        dashboard: &dashboard_vm,
    };

    let rendered = template.render()?;
    Ok(Html(rendered))
}
