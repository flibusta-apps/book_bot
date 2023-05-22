use std::str::FromStr;

use moka::future::Cache;
use regex::Regex;
use strum_macros::EnumIter;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup}, dispatching::dialogue::GetChatId, adaptors::{Throttle, CacheMe},
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
    Book { page: u32 },
    Authors { page: u32 },
    Sequences { page: u32 },
    Translators { page: u32 },
}

impl ToString for SearchCallbackData {
    fn to_string(&self) -> String {
        match self {
            SearchCallbackData::Book { page } => format!("sb_{page}"),
            SearchCallbackData::Authors { page } => format!("sa_{page}"),
            SearchCallbackData::Sequences { page } => format!("ss_{page}"),
            SearchCallbackData::Translators { page } => format!("st_{page}"),
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
            "sb" => Ok(SearchCallbackData::Book { page }),
            "sa" => Ok(SearchCallbackData::Authors { page }),
            "ss" => Ok(SearchCallbackData::Sequences { page }),
            "st" => Ok(SearchCallbackData::Translators { page }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

impl GetPaginationCallbackData for SearchCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        match self {
            SearchCallbackData::Book { .. } => {
                SearchCallbackData::Book { page: target_page }
            }
            SearchCallbackData::Authors { .. } => {
                SearchCallbackData::Authors { page: target_page }
            }
            SearchCallbackData::Sequences { .. } => {
                SearchCallbackData::Sequences { page: target_page }
            }
            SearchCallbackData::Translators { .. } => {
                SearchCallbackData::Translators { page: target_page }
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
                        .map(|text| text.replace(['/', '&', '?'], ""))
                })
                .unwrap_or(None)
        })
        .unwrap_or(None)
}

async fn generic_search_pagination_handler<T, Fut>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    search_data: SearchCallbackData,
    items_getter: fn(query: String, page: u32, allowed_langs: Vec<String>) -> Fut,
    user_langs_cache: Cache<UserId, Vec<String>>,
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
        (Some(chat_id), _, _) => {
            bot.send_message(chat_id, "Повторите поиск сначала").send().await?;
            return Ok(());
        }
        _ => {
            return Ok(());
        }
    };

    let allowed_langs = get_user_or_default_lang_codes(user_id, user_langs_cache).await;

    let page = match search_data {
        SearchCallbackData::Book { page } => page,
        SearchCallbackData::Authors { page } => page,
        SearchCallbackData::Sequences { page } => page,
        SearchCallbackData::Translators { page } => page,
    };

    let mut items_page = match items_getter(query.clone(), page, allowed_langs.clone()).await {
        Ok(v) => v,
        Err(err) => {
            bot
                .send_message(chat_id, "Ошибка! Попробуйте позже :(")
                .send()
                .await?;

            return Err(err);
        }
    };

    if items_page.total_pages == 0 {
        let message_text = match search_data {
            SearchCallbackData::Book { .. } => "Книги не найдены!",
            SearchCallbackData::Authors { .. } => "Авторы не найдены!",
            SearchCallbackData::Sequences { .. } => "Серии не найдены!",
            SearchCallbackData::Translators { .. } => "Переводчики не найдены!",
        };

        bot.send_message(chat_id, message_text).send().await?;
        return Ok(());
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
                bot
                    .send_message(chat_id, "Ошибка! Попробуйте позже :(")
                    .send()
                    .await?;

                return Err(err);
            }
        };
    }

    let formated_items = items_page.format_items();

    let total_pages = items_page.total_pages;

    let footer = format!("\n\nСтраница {page}/{total_pages}");
    let message_text = format!("{formated_items}{footer}");

    let keyboard = generic_get_pagination_keyboard(page, total_pages, search_data, true);

    bot
        .edit_message_text(chat_id, message_id, message_text)
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
}

pub async fn message_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    let message_text = "Что ищем?";

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: "Книгу".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    (SearchCallbackData::Book { page: 1 }).to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Автора".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    (SearchCallbackData::Authors { page: 1 }).to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Серию".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    (SearchCallbackData::Sequences { page: 1 }).to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Переводчика".to_string(),
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    (SearchCallbackData::Translators { page: 1 }).to_string(),
                ),
            }],
        ],
    };

    bot
        .send_message(message.chat.id, message_text)
        .reply_to_message_id(message.id)
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
}

pub fn get_search_handler() -> crate::bots::BotHandler {
    dptree::entry().branch(
        Update::filter_message()
            .endpoint(|message, bot| async move { message_handler(message, bot).await }),
    ).branch(
        Update::filter_callback_query()
            .chain(filter_callback_query::<SearchCallbackData>())
            .endpoint(|cq: CallbackQuery, callback_data: SearchCallbackData, bot: CacheMe<Throttle<Bot>>, user_langs_cache: Cache<UserId, Vec<String>>| async move {
                match callback_data {
                    SearchCallbackData::Book { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_book, user_langs_cache).await,
                    SearchCallbackData::Authors { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_author, user_langs_cache).await,
                    SearchCallbackData::Sequences { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_sequence, user_langs_cache).await,
                    SearchCallbackData::Translators { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_translator, user_langs_cache).await,
                }
            })
    )
}
