use std::{net::SocketAddr, sync::Arc};

use tokio::net::TcpListener;

use crate::{
    application::services::dashboard_service::DashboardCacheConfig,
    application::services::DashboardService,
    config::AppConfig,
    infrastructure::{
        ha::{DemoHaClient, HaHttpClient},
        layout::FsDashboardLayoutRepository,
        web::{build_router, AppState},
    },
    shared::error::{AppError, AppResult},
};

pub async fn run(config: AppConfig) -> AppResult<()> {
    let ha_client: Arc<dyn crate::application::ports::HomeAssistantClient> = if config.demo_mode {
        DemoHaClient::new(&config)
    } else {
        HaHttpClient::new(config.clone())?
    };

    let layout_repo = Arc::new(FsDashboardLayoutRepository::new(
        "./data/dashboard_layout.json",
    ));

    // Build cache configuration from AppConfig (env-driven).
    let cache_config = DashboardCacheConfig {
        default_ttl_secs: config.dashboard_cache_ttl_default_secs,
        light_ttl_secs: config.dashboard_cache_ttl_light_secs,
        switch_ttl_secs: config.dashboard_cache_ttl_switch_secs,
        sensor_ttl_secs: config.dashboard_cache_ttl_sensor_secs,
        climate_ttl_secs: config.dashboard_cache_ttl_climate_secs,
    };

    let dashboard_service = Arc::new(DashboardService::new(
        ha_client,
        layout_repo,
        cache_config,
        config.clone(),
    ));

    let state = AppState {
        dashboard_service: Arc::clone(&dashboard_service),
    };
    let cloned_for_periodic = Arc::clone(&dashboard_service);
    cloned_for_periodic.start_periodic_updates();

    let app = build_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.server_port));
    tracing::info!("HARetroPanel listening on http://{addr}");

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to bind: {e}")))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| AppError::Internal(format!("Server error: {e}")))
}
