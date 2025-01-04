use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::ReplyParameters,
};

use std::error::Error;

use self::{strings::format_registered_message, utils::get_token};

pub mod register;
pub mod strings;
pub mod utils;

pub async fn message_handler(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let from_user = message.clone().from.unwrap();
    let text = message.text().unwrap_or("");

    let result = register::register(from_user.id, text).await;

    let message_text = match result {
        register::RegisterStatus::Success { ref username } => format_registered_message(username),
        register::RegisterStatus::RegisterFail => strings::ALREADY_REGISTERED.to_string(),
        register::RegisterStatus::LimitExtended => strings::LIMIT_EXTENDED_MESSAGE.to_string(),
        register::RegisterStatus::WrongToken => strings::ERROR_MESSAGE.to_string(),
    };

    bot.send_message(message.chat.id, message_text)
        .reply_parameters(ReplyParameters::new(message.id))
        .await?;

    Ok(())
}

pub fn get_manager_handler() -> Handler<
    'static,
    dptree::di::DependencyMap,
    Result<(), Box<dyn Error + Send + Sync>>,
    teloxide::dispatching::DpHandlerDescription,
> {
    Update::filter_message().branch(
        Message::filter_text()
            .chain(dptree::filter(|message: Message| {
                get_token(message.text().unwrap()).is_some()
            }))
            .endpoint(message_handler),
    )
}
