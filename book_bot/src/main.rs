use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use sentry::integrations::debug_images::DebugImagesIntegration;
use sentry::types::Dsn;
use sentry::ClientOptions;
use sentry_tracing::EventFilter;
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

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    bots_manager::BotsManager::start(running).await;
}
