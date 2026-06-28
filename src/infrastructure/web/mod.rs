pub mod handlers;
pub mod routes;
pub mod viewhelpers;
pub mod viewmodels;

use std::sync::Arc;

use crate::application::services::DashboardService;

#[derive(Clone)]
pub struct AppState {
    pub dashboard_service: Arc<DashboardService>,
}

// Re-export router builder for convenient use in bootstrap
pub use routes::build_router;
