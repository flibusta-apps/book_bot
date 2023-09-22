use teloxide::{types::Message, adaptors::{CacheMe, Throttle}, Bot};

use crate::{bots::{BotHandlerInternal, approved_bot::modules::support::support_command_handler}, bots_manager::CHAT_DONATION_NOTIFICATIONS_CACHE};

use super::user_settings::{is_need_donate_notifications, mark_donate_notification_sent};


pub async fn send_donation_notification(
    bot: CacheMe<Throttle<Bot>>,
    message: Message,
) -> BotHandlerInternal {
    if CHAT_DONATION_NOTIFICATIONS_CACHE.get(&message.chat.id).await.is_some() {
        return Ok(());
    } else if !is_need_donate_notifications(message.chat.id).await? {
        CHAT_DONATION_NOTIFICATIONS_CACHE.insert(message.chat.id, ()).await;
        return Ok(());
    }

    CHAT_DONATION_NOTIFICATIONS_CACHE.insert(message.chat.id, ()).await;
    mark_donate_notification_sent(message.chat.id).await?;

    support_command_handler(message, bot).await?;

    Ok(())
}
