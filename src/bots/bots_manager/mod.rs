use teloxide::prelude::*;

use std::error::Error;

use self::{strings::format_registered_message, utils::get_token};
use crate::config;

pub mod register;
pub mod strings;
pub mod utils;

pub async fn message_handler(
    message: Message,
    bot: AutoSend<Bot>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let from_user = message.from().unwrap();
    let text = message.text().unwrap_or("");

    let result = register::register(from_user.id, text).await;

    let message_text = match result {
        register::RegisterStatus::Success { ref username } => format_registered_message(&username),
        register::RegisterStatus::NoToken => strings::HELP_MESSAGE.to_string(),
        register::RegisterStatus::WrongToken => strings::ERROR_MESSAGE.to_string(),
        register::RegisterStatus::RegisterFail => strings::ALREADY_REGISTERED.to_string(),
    };

    #[allow(unused_must_use)]
    {
        bot.send_message(message.chat.id, message_text)
            .reply_to_message_id(message.id)
            .await;
    }

    if let register::RegisterStatus::Success { .. } = result {
        #[allow(unused_must_use)]
        {
            bot.send_message(
                config::CONFIG.admin_id.clone(),
                strings::BOT_REGISTERED_TO_ADMIN,
            )
            .await;
        }
    }

    return Ok(());
}

pub fn get_manager_handler() -> Handler<
    'static,
    dptree::di::DependencyMap,
    Result<(), Box<dyn Error + Send + Sync>>,
    teloxide::dispatching::DpHandlerDescription,
> {
    Update::filter_message().branch(
        Message::filter_text()
            .chain(dptree::filter(|message: Message| { get_token(message.text().unwrap()).is_some() })).endpoint(message_handler),
    )
}
