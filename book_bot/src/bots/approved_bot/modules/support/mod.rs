use crate::bots::BotHandlerInternal;
use book_bot_macros::log_handler;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::UpdateFilterExt,
    prelude::*,
    utils::command::BotCommands,
};

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum SupportCommand {
    Support,
    Donate,
}

#[log_handler("support")]
pub async fn support_command_handler(
    message: Message,
    bot: &CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let username = match message.from.as_ref() {
        Some(user) if !user.is_bot => escape_html(&user.first_name),
        Some(user) if user.is_bot => match message.reply_to_message() {
            Some(v) => match &v.from {
                Some(v) => escape_html(&v.first_name),
                None => "пользователь".to_string(),
            },
            None => "пользователь".to_string(),
        },
        _ => "пользователь".to_string(),
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
        .endpoint(|message: Message, bot: CacheMe<Throttle<Bot>>| async move {
            support_command_handler(message, &bot).await
        })
}
