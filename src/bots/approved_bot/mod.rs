pub mod modules;
pub mod services;
mod tools;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::BotCommand,
};

use crate::{
    bots::approved_bot::services::user_settings::{
        create_or_update_user_settings, get_user_settings,
    },
    bots_manager::USER_ACTIVITY_CACHE,
};

use self::{
    modules::{
        annotations::get_annotations_handler, book::get_book_handler,
        download::get_download_handler, help::get_help_handler, random::get_random_handler,
        search::get_search_handler, settings::get_settings_handler, support::get_support_handler,
        update_history::get_update_log_handler,
    },
    services::user_settings::{get_user_or_default_lang_codes, update_user_activity},
};

use super::{
    bots_manager::get_manager_handler, ignore_channel_messages, ignore_chat_join_request,
    ignore_chat_member_update, ignore_user_edited_message, BotCommands, BotHandler,
};

async fn _update_activity(me: teloxide::types::Me, user: teloxide::types::User) -> Option<()> {
    if USER_ACTIVITY_CACHE.contains_key(&user.id) {
        return None;
    }

    tokio::spawn(async move {
        let mut update_result = update_user_activity(user.id).await;

        if update_result.is_err() {
            let allowed_langs = get_user_or_default_lang_codes(user.id).await;
            let current = get_user_settings(user.id).await.ok().flatten();
            let default_search = current.as_ref().and_then(|s| s.default_search);

            if create_or_update_user_settings(
                user.id,
                &user.last_name.unwrap_or("".to_string()),
                &user.first_name,
                &user.username.unwrap_or("".to_string()),
                &me.username.clone().unwrap_or("".to_string()),
                allowed_langs,
                default_search,
            )
            .await
            .is_ok()
            {
                update_result = update_user_activity(user.id).await;
            }
        }

        if update_result.is_ok() {
            USER_ACTIVITY_CACHE.insert(user.id, ()).await;
        }
    });

    None
}

fn update_user_activity_handler() -> BotHandler {
    dptree::entry()
        .branch(Update::filter_callback_query().inspect_async(
            |cq: CallbackQuery, bot: CacheMe<Throttle<Bot>>| async move {
                _update_activity(bot.get_me().await.unwrap(), cq.from).await;
            },
        ))
        .branch(Update::filter_message().inspect_async(
            |message: Message, bot: CacheMe<Throttle<Bot>>| async move {
                if let Some(user) = message.from {
                    _update_activity(bot.get_me().await.unwrap(), user).await;
                }
            },
        ))
}

pub fn get_approved_handler() -> (BotHandler, BotCommands) {
    (
        dptree::entry()
            .branch(ignore_channel_messages())
            .branch(ignore_chat_member_update())
            .branch(ignore_user_edited_message())
            .branch(ignore_chat_join_request())
            .branch(update_user_activity_handler())
            .branch(get_help_handler())
            .branch(get_settings_handler())
            .branch(get_support_handler())
            .branch(get_random_handler())
            .branch(get_download_handler())
            .branch(get_annotations_handler())
            .branch(get_book_handler())
            .branch(get_update_log_handler())
            .branch(get_manager_handler())
            .branch(get_search_handler()),
        Some(vec![
            BotCommand {
                command: String::from("random"),
                description: String::from("🎲 Попытать удачу"),
            },
            BotCommand {
                command: String::from("update_log"),
                description: String::from("🔄 Обновления каталога"),
            },
            BotCommand {
                command: String::from("settings"),
                description: String::from("⚙️ Настройки"),
            },
            BotCommand {
                command: String::from("donate"),
                description: String::from("☕️ Поддержать разработчика"),
            },
        ]),
    )
}
