use axum::{extract::State, response::IntoResponse, Json};

use chrono::{DateTime, Timelike};

use crate::shared::error::AppResult;

use crate::infrastructure::web::AppState;

pub async fn get_solar(State(state): State<AppState>) -> AppResult<impl IntoResponse> {
    tracing::debug!("Fetching solar API data");
    let dashboard_state = state
        .dashboard_service
        .get_dashboard_with_refresh(false)
        .await?;

    let cfg = state.dashboard_service.config();

    let solar_entity = dashboard_state
        .entities
        .iter()
        .find(|e| e.id.0 == cfg.solar_entity_id);

    // Parse wattage by extracting the leading numeric value before any whitespace/unit
    let solar_watts = solar_entity
        .and_then(|e| e.value.as_ref())
        .and_then(|v| {
            v.split_whitespace()
                .next()
                .and_then(|n| n.parse::<f64>().ok())
        })
        .unwrap_or(0.0);

    let history = state.dashboard_service.solar_history_points().await;

    let history_minutes = cfg.solar_history_minutes.max(12);
    let mut chart_labels: Vec<String> = Vec::new();
    let mut chart_values: Vec<f64> = Vec::new();

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
                if (hour >= 21 || hour < 6) && *watts <= 0.0 {
                    continue;
                }
                chart_labels.push(format!("{:02}:{:02}", hour, local.minute()));
            } else {
                chart_labels.push(format!("-{}m", age_mins));
            }
            chart_values.push(*watts);
        }
    }

    // Current solar_watts is already included above if non-zero; no separate append needed
    tracing::debug!(
        solar_entity_id = cfg.solar_entity_id,
        solar_entity_value = ?solar_entity.and_then(|e| e.value.as_ref()),
        solar_watts = 0.0,
        history_point_count = history.len(),
        "Solar wattage is 0 — entity may be missing or value not set"
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
    });

    Ok(Json(body))
}
