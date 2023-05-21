use crate::bots::BotHandlerInternal;

use teloxide::{
    prelude::*,
    utils::command::BotCommands, adaptors::{Throttle, CacheMe},
};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum SupportCommand {
    Support,
    Donate
}

pub async fn support_command_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    let username = &message.from().unwrap().first_name;

    let message_text = format!("
Привет, {:?}!

Этот бот существует благодаря пожертвованиям от наших пользователей.
Однако, для его дальнейшего развития и поддержки серверов требуются финансовые средства.
Мы будем очень благодарны за любую сумму пожертвования!

Спасибо!

Тинькофф/Сбербанк:
`+79534966556`
", username);

    bot
        .send_message(message.chat.id, message_text)
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;

    Ok(())
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
