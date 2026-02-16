pub mod callback_data;
pub mod utils;

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
        modules::utils::message_text::is_message_text_equals,
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

use super::utils::pagination::generic_get_pagination_keyboard;

async fn generic_search_pagination_handler<T, P, Fut>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    search_data: SearchCallbackData,
    items_getter: fn(query: String, page: u32, allowed_langs: SmallVec<[SmartString; 3]>) -> Fut,
) -> BotHandlerInternal
where
    T: Format + Clone + Debug,
    P: FormatTitle + Clone + Debug,
    Fut: std::future::Future<Output = anyhow::Result<Page<T, P>>>,
{
    let chat_id = cq.chat_id();
    let user_id = cq.from.id;
    let message_id = cq.message.as_ref().map(|message| message.id());
    let query = get_query(cq.clone());

    let (chat_id, query, message_id) = match (chat_id, query, message_id) {
        (Some(chat_id), Some(query), Some(message_id)) => (chat_id, query, message_id),
        (Some(chat_id), _, _) => {
            bot.send_message(chat_id, "Повторите поиск сначала")
                .send()
                .await?;
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

    let mut items_page = match items_getter(query.clone(), page, allowed_langs.clone()).await {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(chat_id, "Ошибка! Попробуйте позже :(")
                .send()
                .await?;

            return Err(err);
        }
    };

    if items_page.pages == 0 {
        let message_text = match search_data {
            SearchCallbackData::Book { .. } => "Книги не найдены!",
            SearchCallbackData::Authors { .. } => "Авторы не найдены!",
            SearchCallbackData::Sequences { .. } => "Серии не найдены!",
            SearchCallbackData::Translators { .. } => "Переводчики не найдены!",
        };

        bot.send_message(chat_id, message_text).send().await?;
        return Ok(());
    };

    if page > items_page.pages {
        items_page = match items_getter(query, items_page.pages, allowed_langs).await {
            Ok(v) => v,
            Err(err) => {
                bot.send_message(chat_id, "Ошибка! Попробуйте позже :(")
                    .send()
                    .await?;

                return Err(err);
            }
        };
    }

    let formatted_page = items_page.format(page, 4096);
    if is_message_text_equals(cq.message, &formatted_page) {
        return Ok(());
    }

    let keyboard = generic_get_pagination_keyboard(page, items_page.pages, search_data, true);
    match bot
        .edit_message_text(chat_id, message_id, formatted_page)
        .reply_markup(keyboard)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

pub async fn message_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    let query = message.text().map(|t| t.trim()).filter(|t| !t.is_empty());
    let user_id = message.from.as_ref().map(|u| u.id);

    if let (Some(user_id), Some(query)) = (user_id, query) {
        if let Some(default_type) = get_user_default_search(user_id).await {
            let search_data = default_search_to_callback_data(default_type);
            let allowed_langs = get_user_or_default_lang_codes(user_id).await;
            let query_owned = query.to_string();
            let chat_id = message.chat.id;
            let reply_params = ReplyParameters::new(message.id);

            let (formatted, pages) = match &search_data {
                SearchCallbackData::Book { .. } => {
                    match search_book(query_owned, 1, allowed_langs).await {
                        Ok(p) if p.pages == 0 => {
                            bot.send_message(chat_id, "Книги не найдены!")
                                .reply_parameters(reply_params)
                                .send()
                                .await?;
                            return Ok(());
                        }
                        Ok(p) => (p.format(1, 4096), p.pages),
                        Err(_) => {
                            bot.send_message(chat_id, "Ошибка! Попробуйте позже :(")
                                .send()
                                .await?;
                            return Ok(());
                        }
                    }
                }
                SearchCallbackData::Authors { .. } => {
                    match search_author(query_owned, 1, allowed_langs).await {
                        Ok(p) if p.pages == 0 => {
                            bot.send_message(chat_id, "Авторы не найдены!")
                                .reply_parameters(reply_params)
                                .send()
                                .await?;
                            return Ok(());
                        }
                        Ok(p) => (p.format(1, 4096), p.pages),
                        Err(_) => {
                            bot.send_message(chat_id, "Ошибка! Попробуйте позже :(")
                                .send()
                                .await?;
                            return Ok(());
                        }
                    }
                }
                SearchCallbackData::Sequences { .. } => {
                    match search_sequence(query_owned, 1, allowed_langs).await {
                        Ok(p) if p.pages == 0 => {
                            bot.send_message(chat_id, "Серии не найдены!")
                                .reply_parameters(reply_params)
                                .send()
                                .await?;
                            return Ok(());
                        }
                        Ok(p) => (p.format(1, 4096), p.pages),
                        Err(_) => {
                            bot.send_message(chat_id, "Ошибка! Попробуйте позже :(")
                                .send()
                                .await?;
                            return Ok(());
                        }
                    }
                }
                SearchCallbackData::Translators { .. } => {
                    match search_translator(query_owned, 1, allowed_langs).await {
                        Ok(p) if p.pages == 0 => {
                            bot.send_message(chat_id, "Переводчики не найдены!")
                                .reply_parameters(reply_params)
                                .send()
                                .await?;
                            return Ok(());
                        }
                        Ok(p) => (p.format(1, 4096), p.pages),
                        Err(_) => {
                            bot.send_message(chat_id, "Ошибка! Попробуйте позже :(")
                                .send()
                                .await?;
                            return Ok(());
                        }
                    }
                }
            };

            let keyboard = generic_get_pagination_keyboard(1, pages, search_data, true);
            bot.send_message(chat_id, formatted)
                .reply_parameters(reply_params)
                .reply_markup(keyboard)
                .send()
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

    bot.send_message(message.chat.id, message_text)
        .reply_parameters(ReplyParameters::new(message.id))
        .reply_markup(keyboard)
        .send()
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
