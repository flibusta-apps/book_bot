use chrono::{prelude::*, Duration};
use dateparser::parse;

use std::{str::FromStr, vec};

use crate::bots::{
    approved_bot::{services::book_library::get_uploaded_books, tools::filter_callback_query},
    BotHandlerInternal,
};

use regex::Regex;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
    utils::command::BotCommands,
};

use super::utils::{generic_get_pagination_keyboard, GetPaginationCallbackData};

#[derive(BotCommands, Clone)]
#[command(rename = "snake_case")]
enum UpdateLogCommand {
    UpdateLog,
}

#[derive(Clone, Copy)]
struct UpdateLogCallbackData {
    from: Date<Utc>,
    to: Date<Utc>,
    page: u32,
}

impl FromStr for UpdateLogCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(
            r"^update_log_(?P<from>\d{4}-\d{2}-\d{2})_(?P<to>\d{4}-\d{2}-\d{2})_(?P<page>\d+)$",
        )
        .unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let from: Date<Utc> = parse(&caps["from"]).unwrap().date();
        let to: Date<Utc> = parse(&caps["to"]).unwrap().date();
        let page: u32 = caps["page"].parse().unwrap();

        Ok(UpdateLogCallbackData { from, to, page })
    }
}

impl ToString for UpdateLogCallbackData {
    fn to_string(&self) -> String {
        let date_format = "%Y-%m-%d";

        let from = self.from.format(date_format);
        let to = self.to.format(date_format);
        let page = self.page;

        format!("update_log_{from}_{to}_{page}")
    }
}

impl GetPaginationCallbackData for UpdateLogCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        let UpdateLogCallbackData { from, to, .. } = self;
        UpdateLogCallbackData {
            from: from.clone(),
            to: to.clone(),
            page: target_page,
        }
        .to_string()
    }
}

async fn update_log_command(message: Message, bot: AutoSend<Bot>) -> BotHandlerInternal {
    let now = Utc::today();
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
    bot: AutoSend<Bot>,
    update_callback_data: UpdateLogCallbackData,
) -> BotHandlerInternal {
    let message = match cq.message {
        Some(v) => v,
        None => return Ok(()), // TODO: send notification
    };

    let from = update_callback_data.from.format("%d.%m.%Y");
    let to = update_callback_data.to.format("%d.%m.%Y");

    let header = format!("Обновление каталога ({from} - {to}):\n\n");

    let mut items_page = match get_uploaded_books(
        update_callback_data.page,
        update_callback_data.from.format("%Y-%m-%d").to_string(),
        update_callback_data.to.format("%Y-%m-%d").to_string(),
    )
    .await
    {
        Ok(v) => v,
        Err(err) => return Err(err),
    };

    if items_page.total_pages == 0 {
        return match bot
            .send_message(message.chat.id, "Нет новых книг за этот период.")
            .send()
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => Err(Box::new(err)),
        };
    }

    if update_callback_data.page > items_page.total_pages {
        items_page = match get_uploaded_books(
            items_page.total_pages,
            update_callback_data.from.format("YYYY-MM-DD").to_string(),
            update_callback_data.to.format("YYYY-MM-DD").to_string(),
        )
        .await
        {
            Ok(v) => v,
            Err(err) => return Err(err),
        };
    }

    let formated_items = items_page.format_items();

    let page = update_callback_data.page;
    let total_pages = items_page.total_pages;
    let footer = format!("\n\nСтраница {page}/{total_pages}");

    let message_text = format!("{header}{formated_items}{footer}");

    let keyboard = generic_get_pagination_keyboard(1, total_pages, update_callback_data, true);
    match bot
        .edit_message_text(message.chat.id, message.id, message_text)
        .reply_markup(keyboard)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

pub fn get_update_log_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message().branch(
                dptree::entry()
                    .filter_command::<UpdateLogCommand>()
                    .endpoint(|message, bot| async move { update_log_command(message, bot).await }),
            ),
        )
        .branch(
            Update::filter_callback_query().branch(
                dptree::entry()
                    .chain(filter_callback_query::<UpdateLogCallbackData>())
                    .endpoint(
                        |cq: CallbackQuery,
                         bot: AutoSend<Bot>,
                         update_log_data: UpdateLogCallbackData| async move {
                            update_log_pagination_handler(cq, bot, update_log_data).await
                        },
                    ),
            ),
        )
}
