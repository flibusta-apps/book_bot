pub mod bot_manager_client;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::Router;
use teloxide::adaptors::throttle::Limits;
use teloxide::types::BotCommand;
use tokio::time::{sleep, Duration};

use teloxide::{
    dispatching::{update_listeners::webhooks, ShutdownToken},
    prelude::*,
};
use url::Url;

use moka::future::Cache;

use self::bot_manager_client::get_bots;
pub use self::bot_manager_client::{BotCache, BotData, BotStatus};
use crate::config;

#[derive(Clone)]
pub struct AppState {
    pub user_activity_cache: Cache<UserId, bool>,
    pub user_langs_cache: Cache<UserId, Vec<String>>,
    pub chat_donation_notifications_cache: Cache<ChatId, ()>,
}

pub struct BotsManager {
    app_state: AppState,
    next_port: u16,
    bot_port_map: HashMap<u32, u16>,
    bot_shutdown_token_map: HashMap<u32, ShutdownToken>,
}

pub enum BotStartResult {
    Success,
    SuccessWithRouter(Router),
    Failed
}

impl BotsManager {
    pub fn create() -> Self {
        BotsManager {
            app_state: AppState {
                user_activity_cache: Cache::builder()
                    .time_to_live(Duration::from_secs(5 * 60))
                    .max_capacity(16384)
                    .build(),
                user_langs_cache: Cache::builder()
                    .time_to_live(Duration::from_secs(5 * 60))
                    .max_capacity(16384)
                    .build(),
                chat_donation_notifications_cache: Cache::builder()
                    .time_to_live(Duration::from_secs(24 * 60 * 60))
                    .max_capacity(32768)
                    .build(),
            },
            next_port: 8000,
            bot_port_map: HashMap::new(),
            bot_shutdown_token_map: HashMap::new(),
        }
    }

    async fn start_bot(&mut self, bot_data: &BotData, is_first_start: bool) -> BotStartResult {
        let bot = Bot::new(bot_data.token.clone())
            .set_api_url(config::CONFIG.telegram_bot_api.clone())
            .throttle(Limits::default())
            .cache_me();

        let token = bot.inner().inner().token();
        let port = self
            .bot_port_map
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

        let (handler, commands) = crate::bots::get_bot_handler();

        let set_command_result = match commands {
            Some(v) => bot.set_my_commands::<Vec<BotCommand>>(v).send().await,
            None => bot.delete_my_commands().send().await,
        };
        match set_command_result {
            Ok(_) => (),
            Err(err) => log::error!("{:?}", err),
        }

        let mut dispatcher = Dispatcher::builder(bot.clone(), handler)
            .dependencies(dptree::deps![bot_data.cache, self.app_state.clone()])
            .build();

        let shutdown_token = dispatcher.shutdown_token();
            self.bot_shutdown_token_map
                .insert(bot_data.id, shutdown_token);

        if is_first_start {
            let (listener, router) = match webhooks::axum_to_router(bot.clone(), webhooks::Options::new(addr, url)).await {
                Ok(v) => (v.0, v.2),
                Err(err) => {
                    log::warn!("{}", err);

                    return BotStartResult::Failed;
                },
            };

            tokio::spawn(async move {
                dispatcher
                    .dispatch_with_listener(
                        listener,
                        LoggingErrorHandler::with_custom_text("An error from the update listener"),
                    )
                    .await;
            });

            BotStartResult::SuccessWithRouter(router)
        } else {
            let listener = match webhooks::axum(bot.clone(), webhooks::Options::new(addr, url)).await {
                Ok(v) => v,
                Err(err) => {
                    log::warn!("{}", err);

                    return BotStartResult::Failed;
                },
            };

            tokio::spawn(async move {
                dispatcher
                    .dispatch_with_listener(
                        listener,
                        LoggingErrorHandler::with_custom_text("An error from the update listener"),
                    )
                    .await;
            });

            BotStartResult::Success
        }
    }

    async fn sd_token(token: &ShutdownToken) {
        for _ in 1..10 {
            if let Ok(v) = token.clone().shutdown() {
                return v.await;
            }

            sleep(Duration::from_millis(100)).await;
        }
    }

    async fn update_data(&mut self, bots_data: Vec<BotData>, is_first_start: bool) -> Vec<Router> {
        let mut routers: Vec<Router> = vec![];

        for bot_data in bots_data.iter() {
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.bot_port_map.entry(bot_data.id)
            {
                e.insert(self.next_port);

                if !is_first_start {
                    self.next_port += 1;
                }

                match self.start_bot(bot_data, is_first_start).await {
                    BotStartResult::Success => (),
                    BotStartResult::SuccessWithRouter(router) => {
                        routers.push(router);
                    },
                    BotStartResult::Failed => {
                        self.bot_shutdown_token_map.remove(&bot_data.id);
                    },
                }
            }
        }

        routers
    }

    async fn check(&mut self, is_first_start: bool) -> Option<Vec<Router>> {
        let bots_data = get_bots().await;

        match bots_data {
            Ok(v) => Some(self.update_data(v, is_first_start).await),
            Err(err) => {
                log::info!("{:?}", err);

                None
            }
        }
    }

    async fn stop_all(&mut self) {
        for token in self.bot_shutdown_token_map.values() {
            BotsManager::sd_token(token).await;
        }
    }

    async fn start_axum_server(&mut self) {
        loop {
            let routers = match self.check(true).await {
                Some(v) => v,
                None => continue,
            };

            let mut app = Router::new();

            for router in routers {
                app = app.merge(router);
            }

            let addr = SocketAddr::from(([0, 0, 0, 0], self.next_port));
            self.next_port += 1;

            tokio::spawn(async move {
                log::info!("Start webserver...");
                axum::Server::bind(&addr)
                    .serve(app.into_make_service())
                    .await
                    .unwrap();
                log::info!("Webserver shutdown...")
            });

            return;
        }
    }

    pub async fn start(running: Arc<AtomicBool>) {
        let mut manager = BotsManager::create();

        manager.start_axum_server().await;

        loop {
            if !running.load(Ordering::SeqCst) {
                manager.stop_all().await;
                return;
            }

            sleep(Duration::from_secs(30)).await;

            manager.check(false).await;
        }
    }
}
