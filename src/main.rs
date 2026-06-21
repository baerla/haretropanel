use haretropanel::bootstrap::run;
use haretropanel::config::AppConfig;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn init_tracing(config: &AppConfig) {
    let env_filter = EnvFilter::new(&config.log_level);

    let registry = tracing_subscriber::registry().with(env_filter);

    if let Some(log_file) = &config.log_file {
        let log_path = std::path::Path::new(log_file);
        // Create log directory if it does not exist
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let file_appender = RollingFileAppender::new(Rotation::NEVER, log_path, "haretropanel.log");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        // Leak guard to keep it alive for process lifetime
        std::mem::forget(Box::new(_guard));
        let fmt_layer = fmt::layer().with_writer(non_blocking).with_ansi(false);
        registry.with(fmt_layer).init();
    } else {
        let fmt_layer = fmt::layer().with_writer(std::io::stdout).with_ansi(true);
        registry.with(fmt_layer).init();
    }
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
