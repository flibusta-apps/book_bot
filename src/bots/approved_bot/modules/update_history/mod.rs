pub mod callback_data;
pub mod commands;

use chrono::{prelude::*, Duration};

use crate::bots::{
    approved_bot::{services::book_library::get_uploaded_books, tools::filter_callback_query},
    BotHandlerInternal,
};

use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
};

use self::{callback_data::UpdateLogCallbackData, commands::UpdateLogCommand};

use super::utils::pagination::generic_get_pagination_keyboard;

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

    match bot
        .send_message(message.chat.id, "Обновление каталога:")
        .reply_markup(keyboard)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

async fn update_log_pagination_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    update_callback_data: UpdateLogCallbackData,
) -> BotHandlerInternal {
    let message = match cq.message {
        Some(v) => v,
        None => {
            bot.send_message(cq.from.id, "Ошибка! Попробуйте заново(")
                .send()
                .await?;
            return Ok(());
        }
    };

    let from = update_callback_data.from.format("%d.%m.%Y");
    let to = update_callback_data.to.format("%d.%m.%Y");

    let header = format!("Обновление каталога ({from} - {to}):\n\n");

    let mut items_page = get_uploaded_books(
        update_callback_data.page,
        update_callback_data
            .from
            .format("%Y-%m-%d")
            .to_string()
            .into(),
        update_callback_data
            .to
            .format("%Y-%m-%d")
            .to_string()
            .into(),
    )
    .await?;

    if items_page.pages == 0 {
        bot.send_message(message.chat().id, "Нет новых книг за этот период.")
            .send()
            .await?;
        return Ok(());
    }

    if update_callback_data.page > items_page.pages {
        items_page = get_uploaded_books(
            items_page.pages,
            update_callback_data
                .from
                .format("%Y-%m-%d")
                .to_string()
                .into(),
            update_callback_data
                .to
                .format("%Y-%m-%d")
                .to_string()
                .into(),
        )
        .await?;
    }

    let page = update_callback_data.page;
    let total_pages = items_page.pages;

    let formatted_page = items_page.format(page, 4096);

    let message_text = format!("{header}{formatted_page}");

    let keyboard = generic_get_pagination_keyboard(page, total_pages, update_callback_data, true);
    bot.edit_message_text(message.chat().id, message.id(), message_text)
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
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
