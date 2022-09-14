mod approved_bot;

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

fn get_pending_handler() -> BotHandler {
    let handler = |msg: Message, bot: AutoSend<Bot>| async move {
        let message_text = "
        Бот зарегистрирован, но не подтвержден администратором! \
        Подтверждение занимает примерно 12 часов.
        ";

        bot.send_message(msg.chat.id, message_text).await?;
        Ok(())
    };

    Update::filter_message().chain(dptree::endpoint(handler))
}

fn get_blocked_handler() -> BotHandler {
    let handler = |msg: Message, bot: AutoSend<Bot>| async move {
        let message_text = "Бот заблокирован!";

        bot.send_message(msg.chat.id, message_text).await?;
        Ok(())
    };

    Update::filter_message().chain(dptree::endpoint(handler))
}

pub fn get_bot_handler(status: crate::bots_manager::BotStatus) -> (BotHandler, BotCommands) {
    match status {
        crate::bots_manager::BotStatus::Pending => (get_pending_handler(), None),
        crate::bots_manager::BotStatus::Approved => approved_bot::get_approved_handler(),
        crate::bots_manager::BotStatus::Blocked => (get_blocked_handler(), None),
    }
}
