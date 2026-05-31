use haretropanel::bootstrap::run;
use haretropanel::config::{AppConfig, LogRotation};

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn init_tracing(config: &AppConfig) {
    // Create log directory if it does not exist
    std::fs::create_dir_all(&config.log_dir).ok();

    let rotation = match config.log_rotation {
        LogRotation::Daily => Rotation::DAILY,
        LogRotation::Hourly => Rotation::HOURLY,
        LogRotation::Never => Rotation::NEVER,
    };

    let file_appender: RollingFileAppender =
        RollingFileAppender::new(rotation, &config.log_dir, "haretropanel.log");

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Keep guard alive for the whole program lifetime
    Box::leak(Box::new(guard));

    let env_filter = EnvFilter::new(&config.log_level);

    let fmt_layer = fmt::layer().with_writer(non_blocking).with_ansi(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

#[tokio::main]
async fn main() {
    // Load config first so we can initialize logging properly
    let config = match AppConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load config: {e}");
            std::process::exit(1);
        }
    };

    init_tracing(&config);

    if let Err(e) = run(config).await {
        tracing::error!("HARetroPanel exited with error: {e}");
    }
}
