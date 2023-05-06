use crate::bots::BotHandlerInternal;

use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
    utils::command::BotCommands, adaptors::{Throttle, CacheMe},
};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum SupportCommand {
    Support,
}

pub async fn support_command_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    const MESSAGE_TEXT: &str = "
[Лицензии](https://github.com/flibusta-apps/book_bot/blob/main/LICENSE.md)

[Исходный код](https://github.com/flibusta-apps)
    ";

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton {
            kind: teloxide::types::InlineKeyboardButtonKind::Url(
                url::Url::parse("https://kurbezz.github.io/Kurbezz/").unwrap(),
            ),
            text: String::from("☕️ Поддержать разработчика"),
        }]],
    };

    match bot
        .send_message(message.chat.id, MESSAGE_TEXT)
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .reply_markup(keyboard)
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

pub fn get_support_handler() -> crate::bots::BotHandler {
    dptree::entry().branch(
        Update::filter_message().branch(
            dptree::entry().filter_command::<SupportCommand>().endpoint(
                |message, bot| async move { support_command_handler(message, bot).await },
            ),
        ),
    )
}
