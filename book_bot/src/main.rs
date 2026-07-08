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
        .with(filter::LevelFilter::INFO)
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
