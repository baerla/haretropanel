use axum::{
    routing::get,
    Router,
};
use tower_http::trace::TraceLayer;

use crate::infrastructure::web::{
    handlers::{
        dashboard_handler::{get_dashboard, post_run_script, post_toggle, get_redirect_to_root},
        settings_handler::{get_entity_settings, post_entity_settings},
    },
    AppState,
};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(get_dashboard))
        .route("/toggle",get(get_redirect_to_root).post(post_toggle),)
        .route("/run_script",get(get_redirect_to_root).post(post_run_script),)
        .route("/settings/entities",get(get_entity_settings).post(post_entity_settings),)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
