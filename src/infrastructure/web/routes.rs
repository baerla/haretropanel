use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::infrastructure::web::{
    handlers::{
        dashboard_handler::{get_dashboard, get_redirect_to_root, post_run_script, post_toggle},
        settings_handler::{get_entity_settings, post_entity_settings},
        solar_api::get_solar,
        websocket_handler::ws_solar,
    },
    AppState,
};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(get_dashboard))
        .route("/toggle", get(get_redirect_to_root).post(post_toggle))
        .route(
            "/run_script",
            get(get_redirect_to_root).post(post_run_script),
        )
        .route(
            "/settings/entities",
            get(get_entity_settings).post(post_entity_settings),
        )
        .route("/api/solar", get(get_solar))
        .route("/ws/solar", get(ws_solar))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
