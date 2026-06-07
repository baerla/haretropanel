pub mod ports;
pub mod services;

// Re-export commonly used application layer types
pub use ports::HomeAssistantClient;
pub use services::{DashboardLayoutRepository, DashboardService};
