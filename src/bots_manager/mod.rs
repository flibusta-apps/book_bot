pub mod axum_server;
pub mod bot_manager_client;
pub mod closable_sender;
pub mod internal;
pub mod utils;

use once_cell::sync::Lazy;
use smartstring::alias::String as SmartString;
use teloxide::adaptors::throttle::Limits;
use teloxide::stop::{StopFlag, StopToken};
use tokio::task::JoinSet;
use tracing::log;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use smallvec::SmallVec;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};

use teloxide::prelude::*;

use moka::future::Cache;

use crate::bots_manager::bot_manager_client::delete_bot;

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

pub static WEBHOOK_CHECK_ERRORS_COUNT: Lazy<Cache<u32, u32>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(600))
        .max_capacity(128)
        .build()
});

type StopTokenWithSender = (
    StopToken,
    StopFlag,
    ClosableSender<Result<Update, std::convert::Infallible>>,
);

pub static BOTS_ROUTES: Lazy<Cache<String, StopTokenWithSender>> = Lazy::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(60 * 60))
        .max_capacity(100)
        .eviction_listener(|token, value: StopTokenWithSender, _cause| {
            log::info!("Stop Bot(token={token})!");

            let (stop_token, _stop_flag, mut sender) = value;

            stop_token.stop();
            sender.close();
        })
        .build()
});

pub static BOTS_DATA: Lazy<Cache<String, BotData>> = Lazy::new(|| Cache::builder().build());
pub static INITED_BOTS_IDS: Lazy<Cache<u32, ()>> = Lazy::new(|| Cache::builder().build());

pub struct BotsManager;

impl BotsManager {
    async fn check_bots_data(bots: &[BotData]) {
        for bot_data in bots.iter() {
            if BOTS_DATA.contains_key(&bot_data.token) {
                continue;
            }

            let bot_data: BotData = bot_data.clone();

            BOTS_DATA.insert(bot_data.token.clone(), bot_data).await;
        }
    }

    async fn check_uninited(bots_data: &[BotData]) {
        let semaphore = Arc::new(Semaphore::const_new(5));
        let mut set_webhook_tasks = JoinSet::new();

        for bot_data in bots_data.iter() {
            if INITED_BOTS_IDS.contains_key(&bot_data.id) {
                continue;
            }

            let bot_data: BotData = bot_data.clone();

            let semaphore = semaphore.clone();
            set_webhook_tasks.spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();

                let webhook_status = set_webhook(&bot_data).await;

                if webhook_status {
                    INITED_BOTS_IDS.insert(bot_data.id, ()).await;
                }

                drop(_permit);
            });
        }

        loop {
            if set_webhook_tasks.join_next().await.is_none() {
                break;
            }
        }
    }

    async fn check(only_bot_data: bool) {
        let bots_data = get_bots().await;

        let bots_data = match bots_data {
            Ok(v) => v,
            Err(err) => {
                log::info!("{err:?}");
                return;
            }
        };

        let _ = BotsManager::check_bots_data(&bots_data).await;

        if !only_bot_data {
            let _ = BotsManager::check_uninited(&bots_data).await;
        }
    }

    pub async fn stop_all() {
        for (_, (stop_token, _, _)) in BOTS_ROUTES.iter() {
            stop_token.stop();
        }

        BOTS_ROUTES.invalidate_all();

        sleep(Duration::from_secs(5)).await;
    }

    pub async fn check_pending_updates() {
        for (token, bot_data) in BOTS_DATA.iter() {
            let error_count = WEBHOOK_CHECK_ERRORS_COUNT
                .get(&bot_data.id)
                .await
                .unwrap_or(0);

            if error_count >= 3 {
                continue;
            }

            let bot = Bot::new(token.clone().as_str()).throttle(Limits::default());

            let result = bot.get_webhook_info().send().await;

            match result {
                Ok(webhook_info) => {
                    if webhook_info.pending_update_count == 0 {
                        continue;
                    }

                    if webhook_info.last_error_message.is_some() {
                        log::error!("Webhook last error: {:?}", webhook_info.last_error_message);

                        set_webhook(&bot_data).await;
                    }
                }
                Err(err) => {
                    let error_message = err.to_string();

                    if error_message.contains("Invalid bot token") {
                        BOTS_DATA.invalidate(token.as_str()).await;
                        if let Err(d_err) = delete_bot(bot_data.id).await {
                            log::error!("Error deleting bot {}: {:?}", bot_data.id, d_err);
                        };
                        continue;
                    }

                    log::error!("Error getting webhook info: {error_message}");

                    WEBHOOK_CHECK_ERRORS_COUNT
                        .insert(bot_data.id, error_count + 1)
                        .await;
                }
            }
        }
    }

    pub async fn start(running: Arc<AtomicBool>) {
        BotsManager::check(true).await;

        start_axum_server(running.clone()).await;

        let mut tick_number: i32 = 0;

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            if !running.load(Ordering::SeqCst) {
                BotsManager::stop_all().await;
                return;
            }

            if tick_number % 30 == 0 {
                BotsManager::check(false).await;
            }

            if tick_number % 180 == 60 {
                BotsManager::check_pending_updates().await;
            }

            tick_number = (tick_number + 1) % 180;
        }
    }
}
