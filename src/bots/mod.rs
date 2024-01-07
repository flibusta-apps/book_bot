mod approved_bot;
pub mod bots_manager;

use std::error::Error;

use teloxide::prelude::*;

pub type BotHandlerInternal = Result<(), Box<dyn Error + Send + Sync>>;

type BotHandler = Handler<
    'static,
    dptree::di::DependencyMap,
    BotHandlerInternal,
    teloxide::dispatching::DpHandlerDescription,
>;

type BotCommands = Option<Vec<teloxide::types::BotCommand>>;

fn ignore_channel_messages() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(Update::filter_channel_post().endpoint(|| async { Ok(()) }))
        .branch(Update::filter_edited_channel_post().endpoint(|| async { Ok(()) }))
}

fn ignore_chat_member_update() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(Update::filter_chat_member().endpoint(|| async { Ok(()) }))
        .branch(Update::filter_my_chat_member().endpoint(|| async { Ok(()) }))
}

fn ignore_user_edited_message() -> crate::bots::BotHandler {
    dptree::entry().branch(Update::filter_edited_message().endpoint(|| async { Ok(()) }))
}

pub fn get_bot_handler() -> (BotHandler, BotCommands) {
    approved_bot::get_approved_handler()
}
