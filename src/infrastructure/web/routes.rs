use axum::{Router, routing::get};

use crate::infrastructure::web::handlers::dashboard_handler::get_dashboard;
use crate::infrastructure::web::handlers::settings_handler::get_entity_settings;
use crate::infrastructure::web::handlers::websocket_handler::ws_solar;
use crate::infrastructure::web::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(get_dashboard))
        .route("/settings/entities", get(get_entity_settings))
        .route("/ws/solar", get(ws_solar))
        .with_state(state)
}
