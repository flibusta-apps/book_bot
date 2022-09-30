pub mod modules;
pub mod services;
mod tools;

use teloxide::{prelude::*, types::BotCommand};

use self::modules::{
    annotations::get_annotations_handler, book::get_book_handler, download::get_download_hander,
    help::get_help_handler, random::get_random_hander, search::get_search_hanlder,
    settings::get_settings_handler, support::get_support_handler,
    update_history::get_update_log_handler,
};

use super::{BotCommands, BotHandler, ignore_channel_messages};

pub fn get_approved_handler() -> (BotHandler, BotCommands) {
    (
        dptree::entry()
            .branch(ignore_channel_messages())
            .branch(get_help_handler())
            .branch(get_settings_handler())
            .branch(get_support_handler())
            .branch(get_random_hander())
            .branch(get_download_hander())
            .branch(get_annotations_handler())
            .branch(get_book_handler())
            .branch(get_update_log_handler())
            .branch(get_search_hanlder()),
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
