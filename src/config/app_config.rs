use crate::shared::error::{AppError, AppResult};
use dotenvy::dotenv;
use std::env;
use std::num::NonZeroU64;
use tracing::info;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub server_port: u16,
    pub ha_base_url: String,
    pub ha_token: Option<String>,

    // demo mode
    pub demo_mode: bool,

    // entity ids for the dashboard
    pub solar_entity_id: String,
    pub solar_buffer_top_entity_id: String,
    pub solar_buffer_bottom_entity_id: String,
    pub solar_flow_entity_id: String,
    pub solar_return_entity_id: String,
    pub solar_pump_entity_id: String,
    pub charger_current_entity_id: String,
    pub goe_status_entity_id: String,
    pub goe_energy_entity_id: String,
    pub goe_car_connected_entity_id: String,
    pub goe_charging_entity_id: String,
    pub garage_left_status_entity_id: String,
    pub garage_left_action_entity_id: String,
    pub garage_right_status_entity_id: String,
    pub garage_right_action_entity_id: String,
    pub solar_max_watts: f64,

    // solar history
    pub solar_history_minutes: u64,
    pub solar_sample_secs: u64,

    // go-e energy tracking
    pub goe_energy_stable_secs: u64,
    pub goe_energy_delta_kwh: f64,

    // force fetch interval
    pub force_fetch_interval_secs: u64,

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
        let solar_buffer_top_entity_id =
            env::var("HARETROPANEL_SOLAR_BUFFER_TOP_ENTITY_ID").unwrap_or_default();
        let solar_buffer_bottom_entity_id =
            env::var("HARETROPANEL_SOLAR_BUFFER_BOTTOM_ENTITY_ID").unwrap_or_default();
        let solar_flow_entity_id =
            env::var("HARETROPANEL_SOLAR_FLOW_ENTITY_ID").unwrap_or_default();
        let solar_return_entity_id =
            env::var("HARETROPANEL_SOLAR_RETURN_ENTITY_ID").unwrap_or_default();
        let solar_pump_entity_id =
            env::var("HARETROPANEL_SOLAR_PUMP_ENTITY_ID").unwrap_or_default();
        let charger_current_entity_id = env::var("HARETROPANEL_CHARGER_CURRENT_ENTITY_ID")
            .unwrap_or_else(|_| "sensor.goe_055063_nrg_11".to_string());
        let goe_status_entity_id = env::var("HARETROPANEL_GOE_STATUS_ENTITY_ID")
            .unwrap_or_else(|_| "sensor.goe_055063_modelstatus_value".to_string());
        let goe_energy_entity_id = env::var("HARETROPANEL_GOE_ENERGY_ENTITY_ID")
            .unwrap_or_else(|_| "sensor.goe_055063_eto".to_string());
        let goe_car_connected_entity_id = env::var("HARETROPANEL_GOE_CAR_CONNECTED_ENTITY_ID")
            .unwrap_or_else(|_| "binary_sensor.goe_055063_car_0".to_string());
        let goe_charging_entity_id = env::var("HARETROPANEL_GOE_CHARGING_ENTITY_ID")
            .unwrap_or_else(|_| "binary_sensor.goe_055063_laden_0".to_string());
        let garage_left_status_entity_id = env::var("HARETROPANEL_GARAGE_LEFT_STATUS_ENTITY_ID")
            .unwrap_or_else(|_| "binary_sensor.garage_garage_links_offen".to_string());
        let garage_left_action_entity_id = env::var("HARETROPANEL_GARAGE_LEFT_ACTION_ENTITY_ID")
            .unwrap_or_else(|_| "cover.garage_garagentor_links_bewegen".to_string());
        let garage_right_status_entity_id = env::var("HARETROPANEL_GARAGE_RIGHT_STATUS_ENTITY_ID")
            .unwrap_or_else(|_| "binary_sensor.garage_garage_rechts_offen".to_string());
        let garage_right_action_entity_id = env::var("HARETROPANEL_GARAGE_RIGHT_ACTION_ENTITY_ID")
            .unwrap_or_else(|_| "cover.garage_garagentor_rechts_bewegen".to_string());
        let solar_max_watts = env::var("HARETROPANEL_SOLAR_MAX_WATTS")
            .unwrap_or_else(|_| "9000".to_string())
            .parse()
            .map_err(|e| AppError::Config(format!("Invalid HARETROPANEL_SOLAR_MAX_WATTS: {e}")))?;

        let solar_history_minutes = env::var("HARETROPANEL_SOLAR_HISTORY_MINUTES")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .map_err(|e| AppError::Config(format!("Invalid HARETROPANEL_SOLAR_HISTORY_MINUTES: {e}")))?;
        let solar_sample_secs = env::var("HARETROPANEL_SOLAR_SAMPLE_SECS")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .map_err(|e| AppError::Config(format!("Invalid HARETROPANEL_SOLAR_SAMPLE_SECS: {e}")))?;

        let goe_energy_stable_secs = env::var("HARETROPANEL_GOE_ENERGY_STABLE_SECS")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .map_err(|e| AppError::Config(format!("Invalid HARETROPANEL_GOE_ENERGY_STABLE_SECS: {e}")))?;
        let goe_energy_delta_kwh = env::var("HARETROPANEL_GOE_ENERGY_DELTA_KWH")
            .unwrap_or_else(|_| "0.02".to_string())
            .parse()
            .map_err(|e| AppError::Config(format!("Invalid HARETROPANEL_GOE_ENERGY_DELTA_KWH: {e}")))?;

        let force_fetch_interval_secs = env::var("HARETROPANEL_FORCE_FETCH_INTERVAL_SECS")
            .unwrap_or_else(|_| "120".to_string())
            .parse()
            .map_err(|e| AppError::Config(format!("Invalid HARETROPANEL_FORCE_FETCH_INTERVAL_SECS: {e}")))?;

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

        let dashboard_cache_ttl_light_secs = env::var("HARETROPANEL_CACHE_TTL_LIGHT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok().filter(|v| *v > 0).and_then(NonZeroU64::new))
            .map(|v| v.get());

        let dashboard_cache_ttl_switch_secs = env::var("HARETROPANEL_CACHE_TTL_SWITCH_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok().filter(|v| *v > 0).and_then(NonZeroU64::new))
            .map(|v| v.get());

        let dashboard_cache_ttl_sensor_secs = env::var("HARETROPANEL_CACHE_TTL_SENSOR_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok().filter(|v| *v > 0).and_then(NonZeroU64::new))
            .map(|v| v.get());

        let dashboard_cache_ttl_climate_secs = env::var("HARETROPANEL_CACHE_TTL_CLIMATE_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok().filter(|v| *v > 0).and_then(NonZeroU64::new))
            .map(|v| v.get());

        info!(
            demo_mode,
            solar_entity_id = ?solar_entity_id,
            charger_current_entity_id = ?charger_current_entity_id,
            goe_status_entity_id = ?goe_status_entity_id,
            goe_car_connected_entity_id = ?goe_car_connected_entity_id,
            goe_charging_entity_id = ?goe_charging_entity_id,
            garage_left_status_entity_id = ?garage_left_status_entity_id,
            garage_left_action_entity_id = ?garage_left_action_entity_id,
            garage_right_status_entity_id = ?garage_right_status_entity_id,
            garage_right_action_entity_id = ?garage_right_action_entity_id,
            "Config loaded"
        );

        Ok(Self {
            server_port,
            ha_base_url,
            ha_token,
            demo_mode,
            solar_entity_id,
            solar_buffer_top_entity_id,
            solar_buffer_bottom_entity_id,
            solar_flow_entity_id,
            solar_return_entity_id,
            solar_pump_entity_id,
            charger_current_entity_id,
            goe_status_entity_id,
            goe_energy_entity_id,
            goe_car_connected_entity_id,
            goe_charging_entity_id,
            garage_left_status_entity_id,
            garage_left_action_entity_id,
            garage_right_status_entity_id,
            garage_right_action_entity_id,
            solar_max_watts,
            solar_history_minutes,
            solar_sample_secs,
            goe_energy_stable_secs,
            goe_energy_delta_kwh,
            force_fetch_interval_secs,
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

#[cfg(test)]
mod app_config_tests {
    use super::*;

    /// Move .env out of the way and restore it after the test.
    /// All config tests are in a single function to avoid parallelism issues with .env and env vars.
    #[test]
    fn test_all_config_scenarios() {
        let dotenv_path = ".env";
        let backup_path = ".env.test_backup";
        let dotenv_exists = std::path::Path::new(dotenv_path).exists();
        if dotenv_exists {
            std::fs::rename(dotenv_path, backup_path).unwrap();
        }

        // Helper closures that capture base_vars
        let base_vars = vec![
            "HARETROPANEL_PORT".to_string(),
            "HA_BASE_URL".to_string(),
            "HA_TOKEN".to_string(),
            "HARETROPANEL_LOG_DIR".to_string(),
            "HARETROPANEL_LOG_ROTATION".to_string(),
            "HARETROPANEL_LOG_LEVEL".to_string(),
            "HARETROPANEL_CACHE_TTL_DEFAULT_SECS".to_string(),
            "HARETROPANEL_CACHE_TTL_LIGHT_SECS".to_string(),
            "HARETROPANEL_CACHE_TTL_SWITCH_SECS".to_string(),
            "HARETROPANEL_CACHE_TTL_SENSOR_SECS".to_string(),
            "HARETROPANEL_CACHE_TTL_CLIMATE_SECS".to_string(),
        ];
        let base_vars_clone = base_vars.clone();
        let clear_all = move || {
            for var in &base_vars_clone {
                env::remove_var(var);
            }
        };

        // 1. Minimal required vars + defaults
        env::set_var("HA_TOKEN", "t");
        env::set_var("HA_BASE_URL", "https://ha.example.com");
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.server_port, 8080);
        assert_eq!(cfg.ha_base_url, "https://ha.example.com");
        assert_eq!(cfg.ha_token, Some("t".to_string()));
        assert_eq!(cfg.log_dir, "./logs");
        assert!(matches!(cfg.log_rotation, LogRotation::Daily));
        assert_eq!(cfg.log_level, "haretropanel=info,tower_http=info");
        assert_eq!(cfg.dashboard_cache_ttl_default_secs, 5);
        assert_eq!(cfg.dashboard_cache_ttl_light_secs, None);
        assert_eq!(cfg.dashboard_cache_ttl_switch_secs, None);
        assert_eq!(cfg.dashboard_cache_ttl_sensor_secs, None);
        assert_eq!(cfg.dashboard_cache_ttl_climate_secs, None);
        clear_all();

        // 2. Custom port
        env::set_var("HARETROPANEL_PORT", "9090");
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.server_port, 9090);
        clear_all();

        // 3. Invalid port → error
        env::set_var("HARETROPANEL_PORT", "not_a_number");
        assert!(AppConfig::from_env().is_err());
        clear_all();

        // 4. Log rotation variants
        env::set_var("HARETROPANEL_LOG_ROTATION", "hourly");
        assert!(matches!(AppConfig::from_env().unwrap().log_rotation, LogRotation::Hourly));
        clear_all();

        env::set_var("HARETROPANEL_LOG_ROTATION", "never");
        assert!(matches!(AppConfig::from_env().unwrap().log_rotation, LogRotation::Never));
        clear_all();

        env::set_var("HARETROPANEL_LOG_ROTATION", "monthly"); // unknown → Daily
        assert!(matches!(AppConfig::from_env().unwrap().log_rotation, LogRotation::Daily));
        clear_all();

        env::set_var("HARETROPANEL_LOG_ROTATION", "HOURLY"); // case insensitive
        assert!(matches!(AppConfig::from_env().unwrap().log_rotation, LogRotation::Hourly));
        clear_all();

        // 5. Log dir
        env::set_var("HARETROPANEL_LOG_DIR", "/var/log/haretropanel");
        assert_eq!(AppConfig::from_env().unwrap().log_dir, "/var/log/haretropanel");
        clear_all();

        // 6. Log level
        env::set_var("HARETROPANEL_LOG_LEVEL", "debug");
        assert_eq!(AppConfig::from_env().unwrap().log_level, "debug");
        clear_all();

        // 7. Custom cache TTL
        env::set_var("HARETROPANEL_CACHE_TTL_DEFAULT_SECS", "30");
        assert_eq!(AppConfig::from_env().unwrap().dashboard_cache_ttl_default_secs, 30);
        clear_all();

        // 8. Invalid cache TTL → error
        env::set_var("HARETROPANEL_CACHE_TTL_DEFAULT_SECS", "bad");
        assert!(AppConfig::from_env().is_err());
        clear_all();

        // 9. Per-kind cache TTLs
        env::set_var("HARETROPANEL_CACHE_TTL_LIGHT_SECS", "10");
        env::set_var("HARETROPANEL_CACHE_TTL_SWITCH_SECS", "15");
        env::set_var("HARETROPANEL_CACHE_TTL_SENSOR_SECS", "20");
        env::set_var("HARETROPANEL_CACHE_TTL_CLIMATE_SECS", "25");
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.dashboard_cache_ttl_light_secs, Some(10));
        assert_eq!(cfg.dashboard_cache_ttl_switch_secs, Some(15));
        assert_eq!(cfg.dashboard_cache_ttl_sensor_secs, Some(20));
        assert_eq!(cfg.dashboard_cache_ttl_climate_secs, Some(25));
        clear_all();

        // 10. Mixed: some per-kind overrides, some not
        env::set_var("HARETROPANEL_CACHE_TTL_DEFAULT_SECS", "60");
        env::set_var("HARETROPANEL_CACHE_TTL_LIGHT_SECS", "5");
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.dashboard_cache_ttl_default_secs, 60);
        assert_eq!(cfg.dashboard_cache_ttl_light_secs, Some(5));
        assert_eq!(cfg.dashboard_cache_ttl_sensor_secs, None);
        assert_eq!(cfg.dashboard_cache_ttl_climate_secs, None);
        clear_all();

        // 11. Missing HA_TOKEN is allowed by from_env (returns None)
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.ha_token, None);

        // 12. LogRotation debug
        assert!(format!("{:?}", LogRotation::Daily).contains("Daily"));
        assert!(format!("{:?}", LogRotation::Hourly).contains("Hourly"));
        assert!(format!("{:?}", LogRotation::Never).contains("Never"));

        // 13. Clone
        let cfg = AppConfig::from_env().unwrap();
        let cloned = cfg.clone();
        assert_eq!(cfg.server_port, cloned.server_port);
        assert_eq!(cfg.ha_base_url, cloned.ha_base_url);

        // 14. Debug display
        let debug_str = format!("{cfg:?}");
        assert!(debug_str.contains("AppConfig"));

        // --- Restore state ---
        if dotenv_exists {
            std::fs::rename(backup_path, dotenv_path).unwrap();
        }
        for var in &base_vars {
            env::remove_var(var);
        }
    }
}
