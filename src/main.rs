use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use sentry::ClientOptions;
use sentry::integrations::debug_images::DebugImagesIntegration;
use sentry::types::Dsn;

mod bots;
mod bots_manager;
mod config;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let options = ClientOptions {
            dsn: Some(Dsn::from_str(&config::CONFIG.sentry_dsn).unwrap()),
            default_integrations: false,
            ..Default::default()
        }
        .add_integration(DebugImagesIntegration::new());

    let _guard = sentry::init(options);

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    bots_manager::BotsManager::start(running).await;
}
