use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use teloxide::types::BotCommand;
use tokio::time::{sleep, Duration};

use teloxide::{
    dispatching::{update_listeners::webhooks, ShutdownToken},
    prelude::*,
};
use url::Url;

use serde::Deserialize;

use crate::config;

#[derive(Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum BotStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "blocked")]
    Blocked,
}

#[derive(Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum BotCache {
    #[serde(rename = "original")]
    Original,
    #[serde(rename = "no_cache")]
    NoCache,
}

#[derive(Deserialize, Debug)]
struct BotData {
    id: u32,
    token: String,
    status: BotStatus,
    cache: BotCache,
}

async fn get_bots() -> Result<Vec<BotData>, reqwest::Error> {
    let client = reqwest::Client::new();
    let response = client
        .get(&config::CONFIG.manager_url)
        .header("Authorization", &config::CONFIG.manager_api_key)
        .send()
        .await;

    match response {
        Ok(v) => v.json::<Vec<BotData>>().await,
        Err(err) => Err(err),
    }
}

pub struct BotsManager {
    next_port: u16,
    bot_port_map: HashMap<u32, u16>,
    bot_status_and_cache_map: HashMap<u32, (BotStatus, BotCache)>,
    bot_shutdown_token_map: HashMap<u32, ShutdownToken>,
}

impl BotsManager {
    pub fn create() -> Self {
        BotsManager {
            next_port: 8000,
            bot_port_map: HashMap::new(),
            bot_status_and_cache_map: HashMap::new(),
            bot_shutdown_token_map: HashMap::new(),
        }
    }

    async fn start_bot(&mut self, bot_data: &BotData) {
        let bot = Bot::new(bot_data.token.clone())
            .set_api_url(config::CONFIG.telegram_bot_api.clone())
            .auto_send();

        let token = bot.inner().token();
        let port = self.bot_port_map.get(&bot_data.id).unwrap();

        let addr = ([0, 0, 0, 0], *port).into();

        let host = format!("{}:{}", &config::CONFIG.webhook_base_url, port);
        let url = Url::parse(&format!("{host}/{token}")).unwrap();

        log::info!(
            "Start bot(id={}) with {:?} handler, port {}",
            bot_data.id,
            bot_data.status,
            port
        );

        let listener = webhooks::axum(bot.clone(), webhooks::Options::new(addr, url))
            .await
            .expect("Couldn't setup webhook");

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
            .dependencies(dptree::deps![bot_data.cache])
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
    }

    async fn stop_bot(&mut self, bot_id: u32) {
        let shutdown_token = match self.bot_shutdown_token_map.remove(&bot_id) {
            Some(v) => v,
            None => return,
        };

        for _ in 1..100 {
            match shutdown_token.clone().shutdown() {
                Ok(v) => return v.await,
                Err(_) => (),
            };

            sleep(Duration::from_millis(100)).await;
        }
    }

    async fn update_data(&mut self, bots_data: Vec<BotData>) {
        for bot_data in bots_data.iter() {
            if !self.bot_port_map.contains_key(&bot_data.id) {
                self.bot_port_map.insert(bot_data.id, self.next_port);
                self.next_port += 1;
            }

            match self.bot_status_and_cache_map.get(&bot_data.id) {
                Some(v) => {
                    if *v != (bot_data.status, bot_data.cache) {
                        self.bot_status_and_cache_map
                            .insert(bot_data.id, (bot_data.status, bot_data.cache));
                        self.stop_bot(bot_data.id).await;
                        self.start_bot(bot_data).await;
                    }
                }
                None => {
                    self.bot_status_and_cache_map
                        .insert(bot_data.id, (bot_data.status, bot_data.cache));
                    self.start_bot(bot_data).await;
                }
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
            for _ in 1..100 {
                match token.clone().shutdown() {
                    Ok(v) => {
                        v.await;
                        break;
                    }
                    Err(_) => (),
                }
            }
        }
    }

    pub async fn start(running: Arc<AtomicBool>) {
        let mut manager = BotsManager::create();

        loop {
            manager.check().await;

            for _ in 1..30 {
                sleep(Duration::from_secs(1)).await;

                if !running.load(Ordering::SeqCst) {
                    manager.stop_all().await;
                    return;
                }
            }
        }
    }
}
