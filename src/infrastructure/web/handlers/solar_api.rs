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

    let body = serde_json::json!({
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
    });

    Ok(Json(body))
}
