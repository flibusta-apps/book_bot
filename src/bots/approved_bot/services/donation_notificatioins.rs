use moka::future::Cache;
use teloxide::{types::{ChatId, Message}, adaptors::{CacheMe, Throttle}, Bot};

use crate::bots::{BotHandlerInternal, approved_bot::modules::support::support_command_handler};

use super::user_settings::{is_need_donate_notifications, mark_donate_notification_sended};


pub async fn send_donation_notification(
    bot: CacheMe<Throttle<Bot>>,
    message: Message,
    donation_notification_cache: Cache<ChatId, bool>,
) -> BotHandlerInternal {
    if donation_notification_cache.get(&message.chat.id).is_some() {
        return Ok(());
    } else if !is_need_donate_notifications(message.chat.id).await? {
        donation_notification_cache.insert(message.chat.id, true).await;
        return Ok(());
    }

    donation_notification_cache.insert(message.chat.id, true).await;
    mark_donate_notification_sended(message.chat.id).await?;

    support_command_handler(message, bot).await?;

    Ok(())
}
