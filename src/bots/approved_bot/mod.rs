pub mod modules;
pub mod services;
mod tools;

use moka::future::Cache;
use teloxide::{prelude::*, types::BotCommand, adaptors::{Throttle, CacheMe}};

use crate::{bots::approved_bot::services::user_settings::create_or_update_user_settings, bots_manager::AppState};

use self::{
    modules::{
        annotations::get_annotations_handler, book::get_book_handler,
        download::get_download_hander, help::get_help_handler, random::get_random_hander,
        search::get_search_handler, settings::get_settings_handler, support::get_support_handler,
        update_history::get_update_log_handler,
    },
    services::user_settings::{get_user_or_default_lang_codes, update_user_activity},
};

use super::{ignore_channel_messages, BotCommands, BotHandler, bots_manager::get_manager_handler, ignore_chat_member_update};

async fn _update_activity(me: teloxide::types::Me, user: teloxide::types::User, cache: Cache<UserId, bool>) -> Option<()> {
    if cache.contains_key(&user.id) {
        return None;
    }

    tokio::spawn(async move {
        let mut update_result = update_user_activity(user.id).await;

        if update_result.is_err() {
            let allowed_langs = get_user_or_default_lang_codes(user.id).await;

            if create_or_update_user_settings(
                user.id,
                user.last_name.clone().unwrap_or("".to_string()),
                user.first_name.clone(),
                user.username.clone().unwrap_or("".to_string()),
                me.username.clone().unwrap(),
                allowed_langs,
            ).await.is_ok()
            {
                update_result = update_user_activity(user.id).await;
            }
        }

        if update_result.is_ok() {
            cache.insert(user.id, true).await;
        }
    });

    None
}

fn update_user_activity_handler() -> BotHandler {
    dptree::entry()
        .branch(
            Update::filter_callback_query().chain(dptree::filter_map_async(
                |cq: CallbackQuery, bot: CacheMe<Throttle<Bot>>, app_state: AppState| async move {
                    _update_activity(bot.get_me().await.unwrap(), cq.from, app_state.user_activity_cache).await
                },
            )),
        )
        .branch(Update::filter_message().chain(dptree::filter_map_async(
            |message: Message, bot: CacheMe<Throttle<Bot>>, app_state: AppState| async move {
                match message.from() {
                    Some(user) => _update_activity(bot.get_me().await.unwrap(), user.clone(), app_state.user_activity_cache).await,
                    None => None,
                }
            },
        )))
}

pub fn get_approved_handler() -> (BotHandler, BotCommands) {
    (
        dptree::entry()
            .branch(ignore_channel_messages())
            .branch(ignore_chat_member_update())
            .branch(update_user_activity_handler())
            .branch(get_help_handler())
            .branch(get_settings_handler())
            .branch(get_support_handler())
            .branch(get_random_hander())
            .branch(get_download_hander())
            .branch(get_annotations_handler())
            .branch(get_book_handler())
            .branch(get_update_log_handler())
            .branch(get_manager_handler())
            .branch(get_search_handler()),
        Some(vec![
            BotCommand {
                command: String::from("random"),
                description: String::from("Попытать удачу"),
            },
            BotCommand {
                command: String::from("update_log"),
                description: String::from("Обновления каталога"),
            },
            BotCommand {
                command: String::from("settings"),
                description: String::from("Настройки"),
            },
            BotCommand {
                command: String::from("support"),
                description: String::from("Поддержать разработчика"),
            },
        ]),
    )
}
