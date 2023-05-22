pub mod bot_manager_client;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use teloxide::adaptors::throttle::Limits;
use teloxide::types::BotCommand;
use tokio::time::{sleep, Duration};

use teloxide::{
    dispatching::{update_listeners::webhooks, ShutdownToken},
    prelude::*,
};
use url::Url;

use moka::future::Cache;

use crate::config;
pub use self::bot_manager_client::{BotStatus, BotCache, BotData};
use self::bot_manager_client::get_bots;


#[derive(Clone)]
pub struct AppState {
    pub user_activity_cache: Cache<UserId, bool>,
    pub user_langs_cache: Cache<UserId, Vec<String>>,
}

pub struct BotsManager {
    app_state: AppState,
    next_port: u16,
    bot_port_map: HashMap<u32, u16>,
    bot_status_and_cache_map: HashMap<u32, (BotStatus, BotCache)>,
    bot_shutdown_token_map: HashMap<u32, ShutdownToken>,
}

impl BotsManager {
    pub fn create() -> Self {
        BotsManager {
            app_state: AppState {
                user_activity_cache: Cache::builder()
                    .time_to_live(Duration::from_secs(5 * 60))
                    .max_capacity(4096)
                    .build(),
                user_langs_cache: Cache::builder()
                    .time_to_live(Duration::from_secs(5 * 60))
                    .max_capacity(4096)
                    .build(),
            },
            next_port: 8000,
            bot_port_map: HashMap::new(),
            bot_status_and_cache_map: HashMap::new(),
            bot_shutdown_token_map: HashMap::new(),
        }
    }

    async fn start_bot(&mut self, bot_data: &BotData) -> bool {
        let bot = Bot::new(bot_data.token.clone())
            .set_api_url(config::CONFIG.telegram_bot_api.clone())
            .throttle(Limits::default())
            .cache_me();

        let token = bot.inner().inner().token();
        let port = self.bot_port_map
            .get(&bot_data.id)
            .unwrap_or_else(|| panic!("Can't get bot port!"));

        let addr = ([0, 0, 0, 0], *port).into();

        let host = format!("{}:{}", &config::CONFIG.webhook_base_url, port);
        let url = Url::parse(&format!("{host}/{token}"))
            .unwrap_or_else(|_| panic!("Can't parse webhook url!"));

        log::info!(
            "Start bot(id={}) with {:?} handler, port {}",
            bot_data.id,
            bot_data.status,
            port
        );

        let listener_result = webhooks::axum(bot.clone(), webhooks::Options::new(addr, url)).await;

        let listener = match listener_result {
            Ok(v) => v,
            Err(err) => {
                log::warn!("{}", err);

                return false;
            },
        };

        let (handler, commands) = crate::bots::get_bot_handler(bot_data.status);

        let set_command_result = match commands {
            Some(v) => bot.set_my_commands::<Vec<BotCommand>>(v).send().await,
            None => bot.delete_my_commands().send().await,
        };
        match set_command_result {
            Ok(_) => (),
            Err(err) => log::error!("{:?}", err),
        }

        let mut dispatcher = Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![bot_data.cache, self.app_state.clone()])
            .build();

        let shutdown_token = dispatcher.shutdown_token();
        self.bot_shutdown_token_map
            .insert(bot_data.id, shutdown_token);

        tokio::spawn(async move {
            dispatcher
                .dispatch_with_listener(
                    listener,
                    LoggingErrorHandler::with_custom_text("An error from the update listener"),
                )
                .await;
        });

        true
    }

    async fn sd_token(token: &ShutdownToken) {
        for _ in 1..10 {
            if let Ok(v) = token.clone().shutdown() { return v.await }

            sleep(Duration::from_millis(100)).await;
        }
    }

    async fn stop_bot(&mut self, bot_id: u32) {
        let shutdown_token = match self.bot_shutdown_token_map.remove(&bot_id) {
            Some(v) => v,
            None => return,
        };

        BotsManager::sd_token(&shutdown_token).await;
    }

    async fn update_data(&mut self, bots_data: Vec<BotData>) {
        for bot_data in bots_data.iter() {
            if let std::collections::hash_map::Entry::Vacant(e) = self.bot_port_map.entry(bot_data.id) {
                e.insert(self.next_port);
                self.next_port += 1;
            }

            let result = match self.bot_status_and_cache_map.get(&bot_data.id) {
                Some(v) => {
                    let mut update_result = true;

                    if *v != (bot_data.status, bot_data.cache) {
                        self.bot_status_and_cache_map
                            .insert(bot_data.id, (bot_data.status, bot_data.cache));
                        self.stop_bot(bot_data.id).await;

                        update_result = self.start_bot(bot_data).await;
                    }

                    update_result
                }
                None => {
                    self.bot_status_and_cache_map
                        .insert(bot_data.id, (bot_data.status, bot_data.cache));

                    self.start_bot(bot_data).await
                }
            };

            if !result {
                self.bot_status_and_cache_map.remove(&bot_data.id);
                self.bot_shutdown_token_map.remove(&bot_data.id);
            }
        }
    }

    async fn check(&mut self) {
        let bots_data = get_bots().await;

        match bots_data {
            Ok(v) => self.update_data(v).await,
            Err(err) => log::info!("{:?}", err),
        }
    }

    async fn stop_all(&mut self) {
        for token in self.bot_shutdown_token_map.values() {
            BotsManager::sd_token(token).await;
        }
    }

    pub async fn start(running: Arc<AtomicBool>) {
        let mut manager = BotsManager::create();

        loop {
            manager.check().await;

            if !running.load(Ordering::SeqCst) {
                manager.stop_all().await;
                return;
            }

            sleep(Duration::from_secs(30)).await;
        }
    }
}
