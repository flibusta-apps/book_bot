use std::str::FromStr;

use sentry::integrations::debug_images::DebugImagesIntegration;
use sentry::types::Dsn;
use sentry::ClientOptions;
use sentry_tracing::EventFilter;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;
use tracing::log;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod bots;
mod bots_manager;
mod config;
pub mod handler_metrics;

/// Builds the log filter from `RUST_LOG` (e.g. `debug,tower_http=warn`),
/// falling back to `info` when the variable is unset or fails to parse.
fn build_env_filter(rust_log: Option<String>) -> filter::EnvFilter {
    rust_log
        .and_then(|spec| filter::EnvFilter::try_new(spec).ok())
        .unwrap_or_else(|| filter::EnvFilter::new("info"))
}

#[tokio::main]
async fn main() {
    let _guard = if let Some(dsn_str) = &config::CONFIG.sentry_dsn {
        let dsn = Dsn::from_str(dsn_str).unwrap_or_else(|_| panic!("Cannot parse SENTRY_DSN"));
        let options = ClientOptions {
            dsn: Some(dsn),
            default_integrations: false,
            ..Default::default()
        }
        .add_integration(DebugImagesIntegration::new());
        sentry::init(options)
    } else {
        sentry::init(())
    };

    let sentry_layer = sentry_tracing::layer().event_filter(|md| match md.level() {
        &tracing::Level::ERROR => EventFilter::Event,
        _ => EventFilter::Ignore,
    });

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(build_env_filter(std::env::var("RUST_LOG").ok()))
        .with(sentry_layer)
        .init();

    let (shutdown_tx, shutdown_rx) = watch::channel(());

    tokio::spawn(async move {
        let mut sigterm =
            signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("Received SIGINT, shutting down...");
            }
            _ = sigterm.recv() => {
                log::info!("Received SIGTERM, shutting down...");
            }
        }

        let _ = shutdown_tx.send(());
    });

    bots_manager::BotsManager::start(shutdown_rx).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_info_when_rust_log_is_unset() {
        assert_eq!(build_env_filter(None).to_string(), "info");
    }

    #[test]
    fn honors_rust_log_override() {
        // `EnvFilter`'s `Display` impl reorders directives deterministically
        // (most-specific target first, bare default level last) rather than
        // preserving input order — verified empirically against
        // tracing-subscriber 0.3.19: `EnvFilter::try_new("debug,tower_http=warn")
        // .unwrap().to_string()` produces "tower_http=warn,debug", not
        // "debug,tower_http=warn".
        assert_eq!(
            build_env_filter(Some("debug,tower_http=warn".to_string())).to_string(),
            "tower_http=warn,debug"
        );
    }

    #[test]
    fn falls_back_to_info_when_rust_log_is_invalid() {
        // A bare word without `=level` (e.g. "not a valid directive!!") is
        // parsed as a target/module name with an implicit default level and
        // never fails — verified empirically. Only a directive with a
        // malformed level after `=` actually returns a parse error.
        assert_eq!(
            build_env_filter(Some("tower_http=notalevel".to_string())).to_string(),
            "info"
        );
    }
}
