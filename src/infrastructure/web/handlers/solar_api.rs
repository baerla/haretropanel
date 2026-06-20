use axum::{extract::State, response::IntoResponse, Json};

use crate::shared::error::AppResult;

use crate::infrastructure::web::AppState;

pub async fn get_solar(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    tracing::debug!("Fetching solar API data");
    let dashboard_state = state
        .dashboard_service
        .get_dashboard_with_refresh(false)
        .await?;

    let cfg = state.dashboard_service.config();

    let solar_watts = state.dashboard_service.parse_solar_watts(&dashboard_state);

    let chart_points = state.dashboard_service.compute_solar_chart().await;
    let chart_labels = chart_points.labels;
    let chart_values = chart_points.values;

    let charger_state = state.dashboard_service.compute_car_state(&dashboard_state).await;

    let buffer_chart_points = state.dashboard_service.compute_buffer_temp_chart().await;
    let buffer_labels = buffer_chart_points.labels;
    let buffer_top_vals: Vec<String> = buffer_chart_points.buffer_top.iter().map(|v| format!("{:.1}", v)).collect();
    let buffer_bottom_vals: Vec<String> = buffer_chart_points.buffer_bottom.iter().map(|v| format!("{:.1}", v)).collect();
    let solar_flow_vals: Vec<String> = buffer_chart_points.solar_flow.iter().map(|v| format!("{:.1}", v)).collect();
    let solar_return_vals: Vec<String> = buffer_chart_points.solar_return.iter().map(|v| format!("{:.1}", v)).collect();
    let pump_status = state.dashboard_service.compute_pump_status(&dashboard_state);

    let make_garage_status = |entity_id: &str, default_name: &str| -> serde_json::Value {
        dashboard_state
            .entities
            .iter()
            .find(|e| e.id.0 == entity_id)
            .map(|e| {
                let is_open = e.is_on;
                let name = e.name.clone();
                let status = if is_open { "Open" } else { "Closed" };
                let action = if is_open { "Close" } else { "Open" };
                let button_class = if is_open {
                    "garage-btn garage-open"
                } else {
                    "garage-btn garage-closed"
                };
                serde_json::json!({
                    "name": name,
                    "status": status,
                    "action": action,
                    "button_class": button_class,
                })
            })
            .unwrap_or_else(|| {
                serde_json::json!({
                    "name": default_name,
                    "status": "Closed",
                    "action": "Open",
                    "button_class": "garage-btn garage-closed",
                })
            })
    };

    tracing::debug!(
        solar_entity_id = cfg.solar_entity_id,
        solar_entity_value = ?dashboard_state
            .entities
            .iter()
            .find(|e| e.id.0 == cfg.solar_entity_id)
            .and_then(|e| e.value.as_ref()),
        solar_watts = solar_watts,
        history_point_count = chart_values.len(),
        "Solar data fetched"
    );

    Ok(Json(serde_json::json!({
        "watts": solar_watts,
        "max_watts": cfg.solar_max_watts,
        "percent": if cfg.solar_max_watts > 0.0 {
            ((solar_watts / cfg.solar_max_watts) * 100.0).round().clamp(0.0, 100.0) as u8
        } else {
            0
        },
        "chart_labels": chart_labels,
        "chart_values": chart_values,
        "charger_amps": charger_state.charger_value,
        "charger_status": charger_state.status,
        "charger_car_state": charger_state.car_state_label,
        "charger_car_state_class": charger_state.car_state_class,
        "charger_car_connected": charger_state.car_connected,
        "charger_charging": charger_state.car_charging,
        "charger_paused": charger_state.paused,
        "garage_left": make_garage_status(cfg.garage_left_status_entity_id.as_str(), "Garage Left"),
        "garage_right": make_garage_status(cfg.garage_right_status_entity_id.as_str(), "Garage Right"),
        "buffer_temps": {
            "labels": buffer_labels,
            "buffer_top": buffer_top_vals,
            "buffer_bottom": buffer_bottom_vals,
            "solar_flow": solar_flow_vals,
            "solar_return": solar_return_vals,
        },
        "pump_status": {
            "pump_on": pump_status.pump_on,
            "is_correct": pump_status.is_correct,
            "status_label": pump_status.status_label,
            "css_class": pump_status.css_class,
        },
    })))
}
