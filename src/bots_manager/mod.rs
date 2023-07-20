pub mod bot_manager_client;

use axum::extract::{State, Path};
use axum::response::IntoResponse;
use axum::routing::post;
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


#[derive(Clone)]
pub struct AppState {
    pub user_activity_cache: Cache<UserId, ()>,
    pub user_langs_cache: Cache<UserId, SmallVec<[SmartString; 3]>>,
    pub chat_donation_notifications_cache: Cache<ChatId, ()>,
}


#[derive(Default, Clone)]
struct ServerState {
    routers: Arc<RwLock<HashMap<String, UnboundedSender<Result<Update, std::convert::Infallible>>>>>,
}

pub struct BotsManager {
    app_state: AppState,

    port: u16,
    stop_data: (StopToken, StopFlag),

    state: ServerState
}

impl BotsManager {
    pub fn create() -> Self {
        BotsManager {
            app_state: AppState {
                user_activity_cache: Cache::builder()
                    .time_to_live(Duration::from_secs(5 * 60))
                    .max_capacity(2048)
                    .build(),
                user_langs_cache: Cache::builder()
                    .time_to_live(Duration::from_secs(5 * 60))
                    .max_capacity(2048)
                    .build(),
                chat_donation_notifications_cache: Cache::builder()
                    .time_to_live(Duration::from_secs(24 * 60 * 60))
                    .max_capacity(2048)
                    .build(),
            },

            port: 8000,
            stop_data: mk_stop_token(),

            state: ServerState {
                routers: Arc::new(RwLock::new(HashMap::new()))
            }
        }
    }

    fn get_listener(&self) -> (UnboundedSender<Result<Update, std::convert::Infallible>>, impl UpdateListener<Err = Infallible>) {
        let (tx, rx): (UpdateSender, _) = mpsc::unbounded_channel();

        let stream = UnboundedReceiverStream::new(rx);

        let listener = StatefulListener::new(
            (stream, self.stop_data.0.clone()),
            tuple_first_mut,
            |state: &mut (_, StopToken)| {
                state.1.clone()
            },
        );

        return (tx, listener);
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
            .dependencies(dptree::deps![bot_data.cache, self.app_state.clone()])
            .build();

        let (tx, listener) = self.get_listener();

        let mut routers = self.state.routers.write().unwrap();
        routers.insert(token.to_string(), tx);

        let host = format!("{}:{}", &config::CONFIG.webhook_base_url, self.port);
        let url = Url::parse(&format!("{host}/{token}"))
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

        return true;
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
            // secret_header: XTelegramBotApiSecretToken,
            input: String,
        ) -> impl IntoResponse {
            // // FIXME: use constant time comparison here
            // if secret_header.0.as_deref() != secret.as_deref().map(str::as_bytes) {
            //     return StatusCode::UNAUTHORIZED;
            // }

            let t1 = routers.read().unwrap();
            let tx = t1.get(&token);

            let tx = match tx {
                Some(tx) => {
                    tx
                    // match tx.get() {
                    //     None => return StatusCode::SERVICE_UNAVAILABLE,
                    //     // Do not process updates after `.stop()` is called even if the server is still
                    //     // running (useful for when you need to stop the bot but can't stop the server).
                    //     // TODO
                    //     // _ if flag.is_stopped() => {
                    //     //     tx.close();
                    //     //     return StatusCode::SERVICE_UNAVAILABLE;
                    //     // }
                    //     Some(tx) => tx,
                    // };
                },
                None => return StatusCode::NOT_FOUND,
            };

            match serde_json::from_str::<Update>(&input) {
                Ok(mut update) => {
                    // See HACK comment in
                    // `teloxide_core::net::request::process_response::{closure#0}`
                    if let UpdateKind::Error(value) = &mut update.kind {
                        *value = serde_json::from_str(&input).unwrap_or_default();
                    }

                    tx.send(Ok(update)).expect("Cannot send an incoming update from the webhook")
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

        let stop_token = self.stop_data.0.clone();
        let stop_flag = self.stop_data.1.clone();
        let state = self.state.clone();
        let port = self.port.clone();

        tokio::spawn(async move {
            log::info!("Start webserver...");

            let addr = SocketAddr::from(([0, 0, 0, 0], port));

            let router = axum::Router::new()
                .route("/:token/", post(telegram_request))
                .layer(TraceLayer::new_for_http())
                .with_state(state);

            axum::Server::bind(&addr)
                .serve(router.into_make_service())
                .with_graceful_shutdown(stop_flag)
                .await
                .map_err(|err| {
                    stop_token.stop();
                    err
                })
                .expect("Axum server error");

            log::info!("Webserver shutdown...");
        });
    }

    pub async fn start(running: Arc<AtomicBool>) {
        let mut manager = BotsManager::create();

        manager.start_axum_server().await;

        loop {
            if !running.load(Ordering::SeqCst) {
                manager.stop_data.0.stop();
                return;
            }

            manager.check().await;

            sleep(Duration::from_secs(30)).await;
        }
    }
}
