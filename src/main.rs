#[macro_use]
extern crate lazy_static;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod bots;
mod bots_manager;
mod config;

#[tokio::main]
async fn main() {
    let _guard = sentry::init(config::CONFIG.sentry_dsn.clone());
    pretty_env_logger::init();

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    tokio::spawn(async move {
        bots_manager::BotsManager::start(running).await;
    })
    .await
    .unwrap();
}
