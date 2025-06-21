use crate::bots::BotHandlerInternal;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::UpdateFilterExt,
    prelude::*,
    utils::command::BotCommands,
};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum SupportCommand {
    Support,
    Donate,
}

pub async fn support_command_handler(
    message: Box<Message>,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let username = match message.clone().from {
        Some(user) => match user.is_bot {
            true => match message.reply_to_message() {
                Some(v) => match &v.from {
                    Some(v) => v.first_name.clone(),
                    None => "пользователь".to_string(),
                },
                None => "пользователь".to_string(),
            },
            false => user.first_name,
        },
        None => "пользователь".to_string(),
    };

    let message_text = format!(
        "
Привет, {username}!

Этот бот существует благодаря пожертвованиям от наших пользователей.
Однако, для его дальнейшего развития и поддержки серверов требуются финансовые средства.
Буду очень благодарен за любую сумму пожертвования!

Спасибо!

Тинькофф:
<pre>5536913820619688</pre>

Сбербанк:
<pre>+79534966556</pre>
"
    );

    bot.send_message(message.chat.id, message_text)
        .parse_mode(teloxide::types::ParseMode::Html)
        .await?;

    Ok(())
}

pub fn get_support_handler() -> crate::bots::BotHandler {
    Update::filter_message()
        .filter_command::<SupportCommand>()
        .endpoint(support_command_handler)
}
