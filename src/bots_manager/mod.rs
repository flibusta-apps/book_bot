pub mod bot_manager_client;

use axum::extract::{State, Path};
use axum::response::IntoResponse;
use axum::routing::post;
use once_cell::sync::Lazy;
use reqwest::StatusCode;
use smartstring::alias::String as SmartString;
use teloxide::stop::{mk_stop_token, StopToken, StopFlag};
use teloxide::update_listeners::{StatefulListener, UpdateListener};
use tokio::sync::mpsc::{UnboundedSender, self};
use tokio_stream::wrappers::UnboundedReceiverStream;
use url::Url;

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use smallvec::SmallVec;
use teloxide::adaptors::throttle::Limits;
use teloxide::types::{BotCommand, UpdateKind};
use tokio::time::{sleep, Duration};
use tower_http::trace::TraceLayer;

use teloxide::prelude::*;

use moka::future::Cache;

use self::bot_manager_client::get_bots;
pub use self::bot_manager_client::{BotCache, BotData};
use crate::config;


type UpdateSender = mpsc::UnboundedSender<Result<Update, std::convert::Infallible>>;


fn tuple_first_mut<A, B>(tuple: &mut (A, B)) -> &mut A {
    &mut tuple.0
}


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


type Routes = Arc<RwLock<HashMap<String, (StopToken, ClosableSender<Result<Update, std::convert::Infallible>>)>>>;


struct ClosableSender<T> {
    origin: std::sync::Arc<std::sync::RwLock<Option<mpsc::UnboundedSender<T>>>>,
}

impl<T> Clone for ClosableSender<T> {
    fn clone(&self) -> Self {
        Self { origin: self.origin.clone() }
    }
}

impl<T> ClosableSender<T> {
    fn new(sender: mpsc::UnboundedSender<T>) -> Self {
        Self { origin: std::sync::Arc::new(std::sync::RwLock::new(Some(sender))) }
    }

    fn get(&self) -> Option<mpsc::UnboundedSender<T>> {
        self.origin.read().unwrap().clone()
    }

    fn close(&mut self) {
        self.origin.write().unwrap().take();
    }
}


#[derive(Default, Clone)]
struct ServerState {
    routers: Routes,
}

pub struct BotsManager {
    port: u16,

    state: ServerState
}

impl BotsManager {
    pub fn create() -> Self {
        BotsManager {
            port: 8000,

            state: ServerState {
                routers: Arc::new(RwLock::new(HashMap::new()))
            }
        }
    }

    fn get_listener(&self) -> (StopToken, StopFlag, UnboundedSender<Result<Update, std::convert::Infallible>>, impl UpdateListener<Err = Infallible>) {
        let (tx, rx): (UpdateSender, _) = mpsc::unbounded_channel();

        let (stop_token, stop_flag) = mk_stop_token();

        let stream = UnboundedReceiverStream::new(rx);

        let listener = StatefulListener::new(
            (stream, stop_token.clone()),
            tuple_first_mut,
            |state: &mut (_, StopToken)| {
                state.1.clone()
            },
        );

        (stop_token, stop_flag, tx, listener)
    }

    async fn start_bot(&mut self, bot_data: &BotData) -> bool {
        let bot = Bot::new(bot_data.token.clone())
            .set_api_url(config::CONFIG.telegram_bot_api.clone())
            .throttle(Limits::default())
            .cache_me();

        let token = bot.inner().inner().token();

        log::info!("Start bot(id={})", bot_data.id);

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
            .dependencies(dptree::deps![bot_data.cache])
            .build();

        let (stop_token, _stop_flag, tx, listener) = self.get_listener();

        {
            let mut routers = self.state.routers.write().unwrap();
            routers.insert(token.to_string(), (stop_token, ClosableSender::new(tx)));
        }

        let host = format!("{}:{}", &config::CONFIG.webhook_base_url, self.port);
        let url = Url::parse(&format!("{host}/{token}/"))
            .unwrap_or_else(|_| panic!("Can't parse webhook url!"));

        match bot.set_webhook(url.clone()).await {
            Ok(_) => (),
            Err(_) => return false,
        }

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

    async fn check(&mut self){
        let bots_data = get_bots().await;

        match bots_data {
            Ok(v) => {
                for bot_data in v.iter() {
                    let need_start = {
                        let routers = self.state.routers.read().unwrap();
                        !routers.contains_key(&bot_data.token)
                    };

                    if need_start {
                        self.start_bot(bot_data).await;
                    }
                }
            },
            Err(err) => {
                log::info!("{:?}", err);
            }
        }
    }

    async fn start_axum_server(&mut self) {
        async fn telegram_request(
            State(ServerState { routers }): State<ServerState>,
            Path(token): Path<String>,
            input: String,
        ) -> impl IntoResponse {

            let routes = routers.read().unwrap();
            let tx = routes.get(&token);

            let (stop_token, r_tx) = match tx {
                Some(tx) => tx,
                None => return StatusCode::NOT_FOUND,
            };

            let tx = match r_tx.get() {
                Some(v) => v,
                None => {
                    stop_token.stop();
                    routers.write().unwrap().remove(&token);
                    return StatusCode::SERVICE_UNAVAILABLE;
                },
            };

            match serde_json::from_str::<Update>(&input) {
                Ok(mut update) => {
                    if let UpdateKind::Error(value) = &mut update.kind {
                        *value = serde_json::from_str(&input).unwrap_or_default();
                    }

                    if let Err(err) = tx.send(Ok(update)) {
                        log::error!("{:?}", err);
                        stop_token.stop();
                        routers.write().unwrap().remove(&token);
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                }
                Err(error) => {
                    log::error!(
                        "Cannot parse an update.\nError: {:?}\nValue: {}\n\
                         This is a bug in teloxide-core, please open an issue here: \
                         https://github.com/teloxide/teloxide/issues.",
                        error,
                        input
                    );
                }
            };

            StatusCode::OK
        }

        let port = self.port;

        let router = axum::Router::new()
            .route("/:token/", post(telegram_request))
            .layer(TraceLayer::new_for_http())
            .with_state(self.state.clone());

        tokio::spawn(async move {
            log::info!("Start webserver...");

            let addr = SocketAddr::from(([0, 0, 0, 0], port));

            axum::Server::bind(&addr)
                .serve(router.into_make_service())
                .await
                .expect("Axum server error");

            log::info!("Webserver shutdown...");
        });
    }

    pub async fn stop_all(self) {
        let routers = self.state.routers.read().unwrap();

        for (stop_token, _) in routers.values() {
            stop_token.stop();
        }

        sleep(Duration::from_secs(5)).await;
    }

    pub async fn start(running: Arc<AtomicBool>) {
        let mut manager = BotsManager::create();

        manager.start_axum_server().await;

        let mut i = 0;

        loop {
            if !running.load(Ordering::SeqCst) {
                manager.stop_all().await;
                return;
            };

            if i == 0 {
                manager.check().await;
            }

            sleep(Duration::from_secs(1)).await;

            i = (i + 1) % 30;
        }
    }
}
