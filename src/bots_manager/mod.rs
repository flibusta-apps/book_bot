pub mod axum_server;
pub mod bot_manager_client;
pub mod closable_sender;
pub mod internal;
pub mod utils;

use once_cell::sync::Lazy;
use smartstring::alias::String as SmartString;
use teloxide::stop::StopToken;
use tracing::log;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use smallvec::SmallVec;
use tokio::time::{self, sleep, Duration};

use teloxide::prelude::*;

use moka::future::Cache;

use self::axum_server::start_axum_server;
use self::bot_manager_client::get_bots;
pub use self::bot_manager_client::{BotCache, BotData};
use self::closable_sender::ClosableSender;

pub static USER_ACTIVITY_CACHE: Lazy<Cache<UserId, ()>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(5 * 60))
        .max_capacity(2048)
        .build()
});

pub static USER_LANGS_CACHE: Lazy<Cache<UserId, SmallVec<[SmartString; 3]>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(5 * 60))
        .max_capacity(2048)
        .build()
});

pub static CHAT_DONATION_NOTIFICATIONS_CACHE: Lazy<Cache<ChatId, ()>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(24 * 60 * 60))
        .max_capacity(2048)
        .build()
});

pub static SERVER_PORT: u16 = 8000;

type StopTokenWithSender = (
    StopToken,
    ClosableSender<Result<Update, std::convert::Infallible>>,
);

pub static BOTS_ROUTES: Lazy<Cache<String, StopTokenWithSender>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(60 * 60))
        .max_capacity(100)
        .eviction_listener(|_token, value: StopTokenWithSender, _cause| {
            let (stop_token, mut sender) = value;

            stop_token.stop();
            sender.close();
        })
        .build()
});

pub static BOTS_DATA: Lazy<Cache<String, BotData>> = Lazy::new(|| Cache::builder().build());

pub struct BotsManager;

impl BotsManager {
    async fn check() {
        let bots_data = get_bots().await;

        match bots_data {
            Ok(v) => {
                for bot_data in v.iter() {
                    BOTS_DATA
                        .insert(bot_data.token.clone(), bot_data.clone())
                        .await;
                }
            }
            Err(err) => {
                log::info!("{:?}", err);
            }
        }
    }

    pub async fn stop_all() {
        for (_, (stop_token, _)) in BOTS_ROUTES.iter() {
            stop_token.stop();
        }

        BOTS_ROUTES.invalidate_all();

        sleep(Duration::from_secs(5)).await;
    }

    pub async fn start(running: Arc<AtomicBool>) {
        start_axum_server().await;

        let mut interval = time::interval(Duration::from_secs(5));

        loop {
            BotsManager::check().await;

            for _i in 0..30 {
                interval.tick().await;

                if !running.load(Ordering::SeqCst) {
                    BotsManager::stop_all().await;
                    return;
                };
            }
        }
    }
}
