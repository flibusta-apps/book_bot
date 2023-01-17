pub mod modules;
pub mod services;
mod tools;

use teloxide::{prelude::*, types::BotCommand};

use crate::bots::approved_bot::services::user_settings::create_or_update_user_settings;

use self::{
    modules::{
        annotations::get_annotations_handler, book::get_book_handler,
        download::get_download_hander, help::get_help_handler, random::get_random_hander,
        search::get_search_handler, settings::get_settings_handler, support::get_support_handler,
        update_history::get_update_log_handler,
    },
    services::user_settings::{get_user_or_default_lang_codes, update_user_activity},
};

use super::{ignore_channel_messages, BotCommands, BotHandler, bots_manager::get_manager_handler};

async fn _update_activity(me: teloxide::types::Me, user: teloxide::types::User) -> Option<()> {
    tokio::spawn(async move {
        if let Err(err) = update_user_activity(user.id).await {
            let allowed_langs = get_user_or_default_lang_codes(user.id).await;

            if let Ok(_) = create_or_update_user_settings(
                user.id,
                user.last_name.clone().unwrap_or("".to_string()),
                user.first_name.clone(),
                user.username.clone().unwrap_or("".to_string()),
                me.username.clone().unwrap(),
                allowed_langs,
            )
            .await
            {
                #[allow(unused_must_use)]
                {
                    update_user_activity(user.id).await;
                }
            }

            log::warn!("{}", err);
        }
    });

    None
}

fn update_user_activity_handler() -> BotHandler {
    dptree::entry()
        .branch(
            Update::filter_callback_query().chain(dptree::filter_map_async(
                |cq: CallbackQuery, me: teloxide::types::Me| async move {
                    _update_activity(me, cq.from).await
                },
            )),
        )
        .branch(Update::filter_message().chain(dptree::filter_map_async(
            |message: Message, me: teloxide::types::Me| async move {
                match message.from() {
                    Some(user) => _update_activity(me, user.clone()).await,
                    None => None,
                }
            },
        )))
}

pub fn get_approved_handler() -> (BotHandler, BotCommands) {
    (
        dptree::entry()
            .branch(ignore_channel_messages())
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
