use crate::shared::error::{AppError, AppResult};
use dotenvy::dotenv;
use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub server_port: u16,
    pub ha_base_url: String,
    pub ha_token: Option<String>,

    // demo mode
    pub demo_mode: bool,

    // entity ids for the dashboard
    pub solar_entity_id: String,
    pub charger_current_entity_id: String,
    pub garage_left_entity_id: String,
    pub garage_right_entity_id: String,
    pub solar_max_watts: f64,

    // logging
    pub log_dir: String,
    pub log_rotation: LogRotation,
    pub log_level: String,

    // dashboard cache (in-memory, per entity kind)
    pub dashboard_cache_ttl_default_secs: u64,
    pub dashboard_cache_ttl_light_secs: Option<u64>,
    pub dashboard_cache_ttl_switch_secs: Option<u64>,
    pub dashboard_cache_ttl_sensor_secs: Option<u64>,
    pub dashboard_cache_ttl_climate_secs: Option<u64>,
}

#[derive(Clone, Debug)]
pub enum LogRotation {
    Daily,
    Hourly,
    Never,
}

impl AppConfig {
    pub fn from_env() -> AppResult<Self> {
        dotenv().ok();

        let server_port = env::var("HARETROPANEL_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .map_err(|e| AppError::Config(format!("Invalid HARETROPANEL_PORT: {e}")))?;

        let ha_base_url =
            env::var("HA_BASE_URL").unwrap_or_else(|_| "http://localhost:8123".to_string());

        let ha_token = env::var("HA_TOKEN").ok();

        let demo_mode = env::var("HARETROPANEL_DEMO_MODE")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase()
            .as_str()
            == "true";

        let solar_entity_id = env::var("HARETROPANEL_SOLAR_ENTITY_ID")
            .unwrap_or_else(|_| "sensor.solar_power".to_string());
        let charger_current_entity_id = env::var("HARETROPANEL_CHARGER_CURRENT_ENTITY_ID")
            .unwrap_or_else(|_| "sensor.goe_charger_current".to_string());
        let garage_left_entity_id = env::var("HARETROPANEL_GARAGE_LEFT_ENTITY_ID")
            .unwrap_or_else(|_| "cover.garage_left".to_string());
        let garage_right_entity_id = env::var("HARETROPANEL_GARAGE_RIGHT_ENTITY_ID")
            .unwrap_or_else(|_| "cover.garage_right".to_string());
        let solar_max_watts = env::var("HARETROPANEL_SOLAR_MAX_WATTS")
            .unwrap_or_else(|_| "9000".to_string())
            .parse()
            .map_err(|e| AppError::Config(format!("Invalid HARETROPANEL_SOLAR_MAX_WATTS: {e}")))?;

        let log_dir = env::var("HARETROPANEL_LOG_DIR").unwrap_or_else(|_| "./logs".to_string());

        let log_rotation = match env::var("HARETROPANEL_LOG_ROTATION")
            .unwrap_or_else(|_| "daily".to_string())
            .to_lowercase()
            .as_str()
        {
            "hourly" => LogRotation::Hourly,
            "never" => LogRotation::Never,
            _ => LogRotation::Daily,
        };

        let log_level = env::var("HARETROPANEL_LOG_LEVEL")
            .unwrap_or_else(|_| "haretropanel=info,tower_http=info".to_string());

        // ---- Dashboard cache TTL config ----
        // Default TTL in seconds (used when no per-kind override exists)
        let dashboard_cache_ttl_default_secs = env::var("HARETROPANEL_CACHE_TTL_DEFAULT_SECS")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .map_err(|e| {
                AppError::Config(format!("Invalid HARETROPANEL_CACHE_TTL_DEFAULT_SECS: {e}"))
            })?;

        // Helper closure to read optional u64 env vars.
        fn read_optional_u64(name: &str) -> AppResult<Option<u64>> {
            match env::var(name) {
                Ok(raw) => {
                    let value = raw
                        .parse()
                        .map_err(|e| AppError::Config(format!("Invalid {name}: {e}")))?;
                    Ok(Some(value))
                }
                Err(_) => Ok(None),
            }
        }

        let dashboard_cache_ttl_light_secs =
            read_optional_u64("HARETROPANEL_CACHE_TTL_LIGHT_SECS")?;
        let dashboard_cache_ttl_switch_secs =
            read_optional_u64("HARETROPANEL_CACHE_TTL_SWITCH_SECS")?;
        let dashboard_cache_ttl_sensor_secs =
            read_optional_u64("HARETROPANEL_CACHE_TTL_SENSOR_SECS")?;
        let dashboard_cache_ttl_climate_secs =
            read_optional_u64("HARETROPANEL_CACHE_TTL_CLIMATE_SECS")?;

        Ok(Self {
            server_port,
            ha_base_url,
            ha_token,
            demo_mode,
            solar_entity_id,
            charger_current_entity_id,
            garage_left_entity_id,
            garage_right_entity_id,
            solar_max_watts,
            log_dir,
            log_rotation,
            log_level,
            dashboard_cache_ttl_default_secs,
            dashboard_cache_ttl_light_secs,
            dashboard_cache_ttl_switch_secs,
            dashboard_cache_ttl_sensor_secs,
            dashboard_cache_ttl_climate_secs,
        })
    }
}
