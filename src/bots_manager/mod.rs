pub mod axum_server;
pub mod bot_manager_client;
pub mod closable_sender;
pub mod internal;
pub mod utils;

use once_cell::sync::Lazy;
use smartstring::alias::String as SmartString;
use teloxide::stop::StopToken;
use teloxide::update_listeners::webhooks;
use tokio::task::JoinSet;
use tracing::log;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use smallvec::SmallVec;
use tokio::time::{self, sleep, Duration};
use tokio::sync::Semaphore;

use teloxide::prelude::*;

use moka::future::Cache;

use self::axum_server::start_axum_server;
use self::bot_manager_client::get_bots;
pub use self::bot_manager_client::{BotCache, BotData};
use self::closable_sender::ClosableSender;
use self::internal::set_webhook;

pub static USER_ACTIVITY_CACHE: Lazy<Cache<UserId, ()>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(30 * 60))
        .max_capacity(4096)
        .build()
});

pub static USER_LANGS_CACHE: Lazy<Cache<UserId, SmallVec<[SmartString; 3]>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(30 * 60))
        .max_capacity(4096)
        .build()
});

pub static CHAT_DONATION_NOTIFICATIONS_CACHE: Lazy<Cache<ChatId, ()>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(24 * 60 * 60))
        .max_capacity(4098)
        .build()
});

type StopTokenWithSender = (
    StopToken,
    ClosableSender<Result<Update, std::convert::Infallible>>,
);

pub static BOTS_ROUTES: Lazy<Cache<String, StopTokenWithSender>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(60 * 60))
        .max_capacity(100)
        .eviction_listener(|token, value: StopTokenWithSender, _cause| {
            log::info!("Stop Bot(token={})!", token);

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

        let bots_data = match bots_data {
            Ok(v) => v,
            Err(err) => {
                log::info!("{:?}", err);
                return;
            }
        };

        let semaphore = Arc::new(Semaphore::const_new(10));
        let mut set_webhook_tasks = JoinSet::new();

        for bot_data in bots_data.iter() {
            if BOTS_DATA.contains_key(&bot_data.token) {
                continue;
            }

            BOTS_DATA
                .insert(bot_data.token.clone(), bot_data.clone())
                .await;

            let bot_data = bot_data.clone();

            let semphore = semaphore.clone();
            set_webhook_tasks.spawn(async move {
                let _permit = semphore.acquire().await.unwrap();

                set_webhook(&bot_data).await;

                drop(_permit);
            });
        }

        loop {
            if set_webhook_tasks.join_next().await.is_none() {
                break;
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

    pub async fn check_pending_updates() {
        for (token, bot_data) in BOTS_DATA.iter() {
            let bot = Bot::new(token.clone().as_str());

            let result = bot.get_webhook_info().send().await;

            match result {
                Ok(webhook_info) => {
                    if webhook_info.pending_update_count != 0 {
                        continue;
                    }

                    if webhook_info.last_error_message.is_some() {
                        log::error!(
                            "Error getting webhook info: {:?}",
                            webhook_info.last_error_message
                        );

                        set_webhook(&bot_data).await;
                    }
                },
                Err(err) => log::error!("Error getting webhook info: {:?}", err),
            }
        }
    }

    pub async fn start(running: Arc<AtomicBool>) {
        start_axum_server(running.clone()).await;

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

            BotsManager::check_pending_updates().await;
        }
    }
}
