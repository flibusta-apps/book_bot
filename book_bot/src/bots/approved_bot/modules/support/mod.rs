use crate::bots::BotHandlerInternal;
use book_bot_macros::log_handler;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::UpdateFilterExt,
    prelude::*,
    utils::command::BotCommands,
};

use crate::bots::approved_bot::modules::utils::telegram_utils::safe_send_message_html;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum SupportCommand {
    Support,
    Donate,
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

#[log_handler("support")]
pub async fn support_command_handler(
    message: Message,
    bot: &CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let username = match message.from.as_ref() {
        Some(user) if !user.is_bot => teloxide::utils::html::escape(&user.first_name),
        Some(user) if user.is_bot => match message.reply_to_message() {
            Some(v) => match &v.from {
                Some(v) => teloxide::utils::html::escape(&v.first_name),
                None => "пользователь".to_string(),
            },
            None => "пользователь".to_string(),
        },
        _ => "пользователь".to_string(),
    };

    let mut message_text = format!(
        "
Привет, {username}!

Этот бот существует благодаря пожертвованиям от наших пользователей.
Однако, для его дальнейшего развития и поддержки серверов требуются финансовые средства.
Буду очень благодарен за любую сумму пожертвования!

Спасибо!
"
    );

    // Payment details come from the environment; a missing/empty value hides the line.
    if let Some(card) = env_non_empty("DONATE_CARD_NUMBER") {
        message_text.push_str(&format!(
            "\nТинькофф:\n<pre>{}</pre>\n",
            teloxide::utils::html::escape(&card)
        ));
    }
    if let Some(phone) = env_non_empty("DONATE_PHONE") {
        message_text.push_str(&format!(
            "\nСбербанк:\n<pre>{}</pre>\n",
            teloxide::utils::html::escape(&phone)
        ));
    }

    safe_send_message_html(bot, message.chat.id, message_text, None).await?;

    Ok(())
}

pub fn get_support_handler() -> crate::bots::BotHandler {
    Update::filter_message()
        .filter_command::<SupportCommand>()
        .endpoint(|message: Message, bot: CacheMe<Throttle<Bot>>| async move {
            support_command_handler(message, &bot).await
        })
}
