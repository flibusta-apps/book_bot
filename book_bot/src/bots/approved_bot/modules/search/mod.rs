pub mod callback_data;
pub mod utils;

use book_bot_macros::log_handler;

use super::utils::constants::*;

use core::fmt::Debug;
use smartstring::alias::String as SmartString;

use smallvec::SmallVec;
use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::dialogue::GetChatId,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, ReplyParameters},
};

use crate::bots::{
    approved_bot::{
        modules::utils::telegram_utils::{safe_send_message, safe_send_message_with_reply},
        services::{
            book_library::{
                formatters::{Format, FormatTitle},
                search_author, search_book, search_sequence, search_translator,
                types::Page,
            },
            user_settings::{get_user_default_search, get_user_or_default_lang_codes},
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use self::{
    callback_data::{default_search_to_callback_data, SearchCallbackData},
    utils::get_query,
};

use super::utils::pagination::{generic_get_pagination_keyboard, paginate, PaginationTexts};

#[log_handler("search")]
async fn generic_search_pagination_handler<T, P, Fut>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    search_data: SearchCallbackData,
    items_getter: fn(query: String, page: u32, allowed_langs: SmallVec<[SmartString; 3]>) -> Fut,
) -> BotHandlerInternal
where
    T: Format + Clone + Debug,
    P: FormatTitle + Clone + Debug,
    Fut: std::future::Future<Output = anyhow::Result<Option<Page<T, P>>>>,
{
    let chat_id = cq.chat_id();
    let user_id = cq.from.id;
    let message_id = cq.message.as_ref().map(|message| message.id());
    let query = get_query(&cq);

    let (chat_id, query, message_id) = match (chat_id, query, message_id) {
        (Some(chat_id), Some(query), Some(message_id)) => (chat_id, query, message_id),
        (Some(chat_id), _, _) => {
            safe_send_message(&bot, chat_id, REPEAT_SEARCH, None).await?;
            return Ok(());
        }
        _ => {
            return Ok(());
        }
    };

    let allowed_langs = get_user_or_default_lang_codes(user_id).await;

    let page = match search_data {
        SearchCallbackData::Book { page } => page,
        SearchCallbackData::Authors { page } => page,
        SearchCallbackData::Sequences { page } => page,
        SearchCallbackData::Translators { page } => page,
    };

    let not_found_text = match search_data {
        SearchCallbackData::Book { .. } => BOOKS_NOT_FOUND,
        SearchCallbackData::Authors { .. } => AUTHORS_NOT_FOUND,
        SearchCallbackData::Sequences { .. } => SEQUENCES_NOT_FOUND,
        SearchCallbackData::Translators { .. } => TRANSLATORS_NOT_FOUND,
    };

    paginate(
        &bot,
        chat_id,
        message_id,
        cq.message,
        page,
        "",
        |p| items_getter(query.clone(), p, allowed_langs.clone()),
        search_data,
        PaginationTexts {
            not_found: not_found_text,
            no_items: not_found_text,
            error_try_later: Some(ERROR_TRY_LATER),
        },
    )
    .await
}

#[log_handler("search")]
pub async fn message_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    let query = message.text().map(|t| t.trim()).filter(|t| !t.is_empty());
    let user_id = message.from.as_ref().map(|u| u.id);

    if let (Some(user_id), Some(query)) = (user_id, query) {
        if let Some(default_type) = get_user_default_search(user_id).await {
            let search_data = default_search_to_callback_data(default_type);
            let allowed_langs = get_user_or_default_lang_codes(user_id).await;
            let query_owned = query.to_string();
            let chat_id = message.chat.id;

            let (formatted, pages) = match &search_data {
                SearchCallbackData::Book { .. } => {
                    match search_book(query_owned, 1, allowed_langs).await {
                        Ok(None) => {
                            safe_send_message_with_reply(
                                &bot,
                                chat_id,
                                BOOKS_NOT_FOUND,
                                ReplyParameters::new(message.id),
                                None,
                            )
                            .await?;
                            return Ok(());
                        }
                        Ok(Some(p)) if p.pages == 0 => {
                            safe_send_message_with_reply(
                                &bot,
                                chat_id,
                                BOOKS_NOT_FOUND,
                                ReplyParameters::new(message.id),
                                None,
                            )
                            .await?;
                            return Ok(());
                        }
                        Ok(Some(p)) => (p.format(1, TELEGRAM_MESSAGE_MAX_LENGTH), p.pages),
                        Err(err) => {
                            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                            return Err(err);
                        }
                    }
                }
                SearchCallbackData::Authors { .. } => {
                    match search_author(query_owned, 1, allowed_langs).await {
                        Ok(None) => {
                            safe_send_message_with_reply(
                                &bot,
                                chat_id,
                                AUTHORS_NOT_FOUND,
                                ReplyParameters::new(message.id),
                                None,
                            )
                            .await?;
                            return Ok(());
                        }
                        Ok(Some(p)) if p.pages == 0 => {
                            safe_send_message_with_reply(
                                &bot,
                                chat_id,
                                AUTHORS_NOT_FOUND,
                                ReplyParameters::new(message.id),
                                None,
                            )
                            .await?;
                            return Ok(());
                        }
                        Ok(Some(p)) => (p.format(1, TELEGRAM_MESSAGE_MAX_LENGTH), p.pages),
                        Err(err) => {
                            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                            return Err(err);
                        }
                    }
                }
                SearchCallbackData::Sequences { .. } => {
                    match search_sequence(query_owned, 1, allowed_langs).await {
                        Ok(None) => {
                            safe_send_message_with_reply(
                                &bot,
                                chat_id,
                                SEQUENCES_NOT_FOUND,
                                ReplyParameters::new(message.id),
                                None,
                            )
                            .await?;
                            return Ok(());
                        }
                        Ok(Some(p)) if p.pages == 0 => {
                            safe_send_message_with_reply(
                                &bot,
                                chat_id,
                                SEQUENCES_NOT_FOUND,
                                ReplyParameters::new(message.id),
                                None,
                            )
                            .await?;
                            return Ok(());
                        }
                        Ok(Some(p)) => (p.format(1, TELEGRAM_MESSAGE_MAX_LENGTH), p.pages),
                        Err(err) => {
                            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                            return Err(err);
                        }
                    }
                }
                SearchCallbackData::Translators { .. } => {
                    match search_translator(query_owned, 1, allowed_langs).await {
                        Ok(None) => {
                            safe_send_message_with_reply(
                                &bot,
                                chat_id,
                                TRANSLATORS_NOT_FOUND,
                                ReplyParameters::new(message.id),
                                None,
                            )
                            .await?;
                            return Ok(());
                        }
                        Ok(Some(p)) if p.pages == 0 => {
                            safe_send_message_with_reply(
                                &bot,
                                chat_id,
                                TRANSLATORS_NOT_FOUND,
                                ReplyParameters::new(message.id),
                                None,
                            )
                            .await?;
                            return Ok(());
                        }
                        Ok(Some(p)) => (p.format(1, TELEGRAM_MESSAGE_MAX_LENGTH), p.pages),
                        Err(err) => {
                            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                            return Err(err);
                        }
                    }
                }
            };

            let keyboard = generic_get_pagination_keyboard(1, pages, search_data, true);
            safe_send_message_with_reply(
                &bot,
                chat_id,
                formatted,
                ReplyParameters::new(message.id),
                Some(keyboard),
            )
            .await?;
            return Ok(());
        }
    }

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

    safe_send_message_with_reply(
        &bot,
        message.chat.id,
        message_text,
        ReplyParameters::new(message.id),
        Some(keyboard),
    )
    .await?;

    Ok(())
}

pub fn get_search_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message()
                .endpoint(|message, bot| async move { message_handler(message, bot).await }),
        )
        .branch(
            Update::filter_callback_query()
                .chain(filter_callback_query::<SearchCallbackData>())
                .endpoint(
                    |cq: CallbackQuery,
                     callback_data: SearchCallbackData,
                     bot: CacheMe<Throttle<Bot>>| async move {
                        match callback_data {
                            SearchCallbackData::Book { .. } => {
                                generic_search_pagination_handler(
                                    cq,
                                    bot,
                                    callback_data,
                                    search_book,
                                )
                                .await
                            }
                            SearchCallbackData::Authors { .. } => {
                                generic_search_pagination_handler(
                                    cq,
                                    bot,
                                    callback_data,
                                    search_author,
                                )
                                .await
                            }
                            SearchCallbackData::Sequences { .. } => {
                                generic_search_pagination_handler(
                                    cq,
                                    bot,
                                    callback_data,
                                    search_sequence,
                                )
                                .await
                            }
                            SearchCallbackData::Translators { .. } => {
                                generic_search_pagination_handler(
                                    cq,
                                    bot,
                                    callback_data,
                                    search_translator,
                                )
                                .await
                            }
                        }
                    },
                ),
        )
}
