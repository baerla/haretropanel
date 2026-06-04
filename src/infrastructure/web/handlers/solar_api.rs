use axum::{extract::State, response::IntoResponse, Json};

use crate::shared::error::AppResult;

use crate::infrastructure::web::AppState;

pub async fn get_solar(
    State(state): State<AppState>,
) -> AppResult<impl IntoResponse> {
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
    let now = std::time::Instant::now();

    let max_points = (cfg.solar_history_minutes * 60 / cfg.solar_sample_secs).max(1) as usize;
    let mut chart_labels: Vec<String> = Vec::new();
    let mut chart_values: Vec<f64> = Vec::new();

    for (idx, (ts, watts)) in history.iter().enumerate() {
        if idx + max_points < history.len() {
            continue;
        }
        let age = now.duration_since(*ts).as_secs();
        let mins = age / 60;
        chart_labels.push(format!("-{}m", mins));
        chart_values.push(*watts);
    }

    // Always append the current solar wattage as the last point
    // so the chart always has at least one visible point
    let now_age_secs = now.elapsed().as_secs();
    chart_labels.push(format!("-{}s", now_age_secs.min(60)));
    chart_values.push(solar_watts);

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
