use std::str::FromStr;

use regex::Regex;
use strum_macros::EnumIter;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup}, dispatching::dialogue::GetChatId,
};

use crate::bots::{
    approved_bot::{
        services::{
            book_library::{
                formaters::Format, search_author, search_book, search_sequence, search_translator,
                types::Page,
            },
            user_settings::get_user_or_default_lang_codes,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use super::utils::{generic_get_pagination_keyboard, GetPaginationCallbackData};

#[derive(Clone, EnumIter)]
pub enum SearchCallbackData {
    SearchBook { page: u32 },
    SearchAuthors { page: u32 },
    SearchSequences { page: u32 },
    SearchTranslators { page: u32 },
}

impl ToString for SearchCallbackData {
    fn to_string(&self) -> String {
        match self {
            SearchCallbackData::SearchBook { page } => format!("sb_{page}"),
            SearchCallbackData::SearchAuthors { page } => format!("sa_{page}"),
            SearchCallbackData::SearchSequences { page } => format!("ss_{page}"),
            SearchCallbackData::SearchTranslators { page } => format!("st_{page}"),
        }
    }
}

impl FromStr for SearchCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^(?P<search_type>s[a|b|s|t])_(?P<page>\d+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let search_type = &caps["search_type"];
        let page: u32 = caps["page"].parse::<u32>().unwrap();

        // Fix for migrate from old bot implementation
        let page: u32 = std::cmp::max(1, page);

        match search_type {
            "sb" => Ok(SearchCallbackData::SearchBook { page }),
            "sa" => Ok(SearchCallbackData::SearchAuthors { page }),
            "ss" => Ok(SearchCallbackData::SearchSequences { page }),
            "st" => Ok(SearchCallbackData::SearchTranslators { page }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

impl GetPaginationCallbackData for SearchCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        match self {
            SearchCallbackData::SearchBook { .. } => {
                SearchCallbackData::SearchBook { page: target_page }
            }
            SearchCallbackData::SearchAuthors { .. } => {
                SearchCallbackData::SearchAuthors { page: target_page }
            }
            SearchCallbackData::SearchSequences { .. } => {
                SearchCallbackData::SearchSequences { page: target_page }
            }
            SearchCallbackData::SearchTranslators { .. } => {
                SearchCallbackData::SearchTranslators { page: target_page }
            }
        }
        .to_string()
    }
}

fn get_query(cq: CallbackQuery) -> Option<String> {
    cq.message
        .map(|message| {
            message
                .reply_to_message()
                .map(|reply_to_message| {
                    reply_to_message
                        .text()
                        .map(|text| text.replace('/', "").replace('&', "").replace('?', ""))
                })
                .unwrap_or(None)
        })
        .unwrap_or(None)
}

async fn generic_search_pagination_handler<T, Fut>(
    cq: CallbackQuery,
    bot: Bot,
    search_data: SearchCallbackData,
    items_getter: fn(query: String, page: u32, allowed_langs: Vec<String>) -> Fut,
) -> BotHandlerInternal
where
    T: Format + Clone,
    Fut: std::future::Future<Output = Result<Page<T>, Box<dyn std::error::Error + Send + Sync>>>,
{
    let chat_id = cq.chat_id();
    let user_id = cq.from.id;
    let message_id = cq.message.as_ref().map(|message| message.id);
    let query = get_query(cq);

    let (chat_id, query, message_id) = match (chat_id, query, message_id) {
        (Some(chat_id), Some(query), Some(message_id)) => {
            (chat_id, query, message_id)
        }
        _ => {
            return match chat_id {
                Some(v) => match bot.send_message(v, "Повторите поиск сначала").send().await
                {
                    Ok(_) => Ok(()),
                    Err(err) => Err(Box::new(err)),
                },
                None => return Ok(()),
            }
        }
    };

    let allowed_langs = get_user_or_default_lang_codes(user_id).await;

    let page = match search_data {
        SearchCallbackData::SearchBook { page } => page,
        SearchCallbackData::SearchAuthors { page } => page,
        SearchCallbackData::SearchSequences { page } => page,
        SearchCallbackData::SearchTranslators { page } => page,
    };

    let mut items_page = match items_getter(query.clone(), page, allowed_langs.clone()).await {
        Ok(v) => v,
        Err(err) => {
            match bot
                .send_message(chat_id, "Ошибка! Попробуйте позже :(")
                .send()
                .await
            {
                Ok(_) => (),
                Err(err) => log::error!("{:?}", err),
            }
            return Err(err);
        }
    };

    if items_page.total_pages == 0 {
        let message_text = match search_data {
            SearchCallbackData::SearchBook { .. } => "Книги не найдены!",
            SearchCallbackData::SearchAuthors { .. } => "Авторы не найдены!",
            SearchCallbackData::SearchSequences { .. } => "Серии не найдены!",
            SearchCallbackData::SearchTranslators { .. } => "Переводчики не найдены!",
        };

        return match bot.send_message(chat_id, message_text).send().await {
            Ok(_) => Ok(()),
            Err(err) => Err(Box::new(err)),
        };
    };

    if page > items_page.total_pages {
        items_page = match items_getter(
            query.clone(),
            items_page.total_pages,
            allowed_langs.clone(),
        )
        .await
        {
            Ok(v) => v,
            Err(err) => {
                match bot
                    .send_message(chat_id, "Ошибка! Попробуйте позже :(")
                    .send()
                    .await
                {
                    Ok(_) => (),
                    Err(err) => log::error!("{:?}", err),
                }
                return Err(err);
            }
        };
    }

    let formated_items = items_page.format_items();

    let total_pages = items_page.total_pages;

    let footer = format!("\n\nСтраница {page}/{total_pages}");
    let message_text = format!("{formated_items}{footer}");

    let keyboard = generic_get_pagination_keyboard(page, total_pages, search_data, true);

    match bot
        .edit_message_text(chat_id, message_id, message_text)
        .reply_markup(keyboard)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

pub async fn message_handler(message: Message, bot: Bot) -> BotHandlerInternal {
    let message_text = "Что ищем?";

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: "Книгу".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    (SearchCallbackData::SearchBook { page: 1 }).to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Автора".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    (SearchCallbackData::SearchAuthors { page: 1 }).to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Серию".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    (SearchCallbackData::SearchSequences { page: 1 }).to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Переводчика".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    (SearchCallbackData::SearchTranslators { page: 1 }).to_string(),
                ),
            }],
        ],
    };

    match bot
        .send_message(message.chat.id, message_text)
        .reply_to_message_id(message.id)
        .reply_markup(keyboard)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

pub fn get_search_handler() -> crate::bots::BotHandler {
    dptree::entry().branch(
        Update::filter_message()
            .endpoint(|message, bot| async move { message_handler(message, bot).await }),
    ).branch(
        Update::filter_callback_query()
            .chain(filter_callback_query::<SearchCallbackData>())
            .endpoint(|cq: CallbackQuery, callback_data: SearchCallbackData, bot: Bot| async move {
                match callback_data {
                    SearchCallbackData::SearchBook { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_book).await,
                    SearchCallbackData::SearchAuthors { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_author).await,
                    SearchCallbackData::SearchSequences { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_sequence).await,
                    SearchCallbackData::SearchTranslators { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_translator).await,
                }
            })
    )
}
