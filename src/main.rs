use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod bots;
mod bots_manager;
mod config;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let _guard = sentry::init(config::CONFIG.sentry_dsn.clone());

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    bots_manager::BotsManager::start(running).await;
}
