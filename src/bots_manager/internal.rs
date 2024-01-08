use teloxide::adaptors::throttle::Limits;
use teloxide::dispatching::Dispatcher;
use teloxide::error_handlers::LoggingErrorHandler;
use teloxide::requests::{Request, Requester, RequesterExt};
use teloxide::stop::StopToken;
use teloxide::stop::{mk_stop_token, StopFlag};
use teloxide::types::{BotCommand, Update};
use teloxide::update_listeners::{StatefulListener, UpdateListener};
use teloxide::{dptree, Bot};

use tokio::sync::mpsc::{self, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;

use tracing::log;
use url::Url;

use std::convert::Infallible;

use crate::bots_manager::BOTS_ROUTES;
use crate::config;

use super::closable_sender::ClosableSender;
use super::utils::tuple_first_mut;
use super::{BotData, SERVER_PORT};

type UpdateSender = mpsc::UnboundedSender<Result<Update, std::convert::Infallible>>;

pub fn get_listener() -> (
    StopToken,
    StopFlag,
    UnboundedSender<Result<Update, std::convert::Infallible>>,
    impl UpdateListener<Err = Infallible>,
) {
    let (tx, rx): (UpdateSender, _) = mpsc::unbounded_channel();

    let (stop_token, stop_flag) = mk_stop_token();

    let stream = UnboundedReceiverStream::new(rx);

    let listener = StatefulListener::new(
        (stream, stop_token.clone()),
        tuple_first_mut,
        |state: &mut (_, StopToken)| state.1.clone(),
    );

    (stop_token, stop_flag, tx, listener)
}

pub async fn set_webhook(bot_data: &BotData) -> bool {
    log::info!("Set webhook Bot(id={})!", bot_data.id);

    let bot = Bot::new(bot_data.token.clone());

    let token = &bot_data.token;

    let host = format!("{}:{}", &config::CONFIG.webhook_base_url, SERVER_PORT);
    let url = Url::parse(&format!("{host}/{token}/"))
        .unwrap_or_else(|_| panic!("Can't parse webhook url!"));

    if bot.set_webhook(url.clone()).await.is_err() {
        return false;
    }

    true
}

pub async fn start_bot(bot_data: &BotData) {
    let bot = Bot::new(bot_data.token.clone())
        .set_api_url(config::CONFIG.telegram_bot_api.clone())
        .throttle(Limits::default())
        .cache_me();

    let token = bot.inner().inner().token();

    log::info!("Start Bot(id={})!", bot_data.id);

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

    let (stop_token, _stop_flag, tx, listener) = get_listener();

    tokio::spawn(async move {
        dispatcher
            .dispatch_with_listener(
                listener,
                LoggingErrorHandler::with_custom_text("An error from the update listener"),
            )
            .await;
    });

    BOTS_ROUTES
        .insert(token.to_string(), (stop_token, ClosableSender::new(tx)))
        .await;
}
