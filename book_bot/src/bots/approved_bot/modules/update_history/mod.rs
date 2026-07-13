pub mod callback_data;
pub mod commands;

use book_bot_macros::log_handler;
use chrono::{prelude::*, Duration};

use crate::bots::{
    approved_bot::{
        modules::utils::{constants::ERROR_TRY_AGAIN, telegram_utils::safe_send_message},
        services::book_library::get_uploaded_books,
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
};

use self::{callback_data::UpdateLogCallbackData, commands::UpdateLogCommand};

use super::utils::pagination::{paginate, PaginationTexts};

#[log_handler("update_history")]
async fn update_log_command(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    let now = Utc::now().date_naive();
    let d3 = now - Duration::days(3);
    let d7 = now - Duration::days(7);
    let d30 = now - Duration::days(30);

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: "За 3 дня".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    UpdateLogCallbackData {
                        from: d3,
                        to: now,
                        page: 1,
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "За 7 дней".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    UpdateLogCallbackData {
                        from: d7,
                        to: now,
                        page: 1,
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "За 30 дней".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    UpdateLogCallbackData {
                        from: d30,
                        to: now,
                        page: 1,
                    }
                    .to_string(),
                ),
            }],
        ],
    };

    safe_send_message(
        &bot,
        message.chat.id,
        "Обновление каталога:",
        Some(keyboard),
    )
    .await
}

#[log_handler("update_history")]
async fn update_log_pagination_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    update_callback_data: UpdateLogCallbackData,
) -> BotHandlerInternal {
    let message = match cq.message.clone() {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), ERROR_TRY_AGAIN, None).await?;
            return Ok(());
        }
    };

    let from = update_callback_data.from.format("%d.%m.%Y");
    let to = update_callback_data.to.format("%d.%m.%Y");

    let header = format!("Обновление каталога ({from} - {to}):\n\n");

    const NO_NEW_BOOKS: &str = "Нет новых книг за этот период.";

    let from = update_callback_data.from;
    let to = update_callback_data.to;

    paginate(
        &bot,
        message.chat().id,
        message.id(),
        cq.message,
        update_callback_data.page,
        &header,
        move |p| {
            get_uploaded_books(
                p,
                from.format("%Y-%m-%d").to_string().into(),
                to.format("%Y-%m-%d").to_string().into(),
            )
        },
        update_callback_data,
        PaginationTexts {
            not_found: NO_NEW_BOOKS,
            no_items: NO_NEW_BOOKS,
            error_try_later: None,
        },
    )
    .await
}

pub fn get_update_log_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message().branch(
                dptree::entry()
                    .filter_command::<UpdateLogCommand>()
                    .endpoint(update_log_command),
            ),
        )
        .branch(
            Update::filter_callback_query().branch(
                dptree::entry()
                    .chain(filter_callback_query::<UpdateLogCallbackData>())
                    .endpoint(update_log_pagination_handler),
            ),
        )
}
