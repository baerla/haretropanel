pub mod dashboard_service;

// Re-export service, layout repository trait, and cache config for convenient access
pub use dashboard_service::{DashboardCacheConfig, DashboardLayoutRepository, DashboardService};
