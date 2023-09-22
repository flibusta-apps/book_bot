pub mod callback_data;
pub mod utils;

use core::fmt::Debug;
use smartstring::alias::String as SmartString;

use smallvec::SmallVec;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup}, dispatching::dialogue::GetChatId, adaptors::{Throttle, CacheMe},
};

use crate::bots::{
    approved_bot::{
        services::{
            book_library::{
                formatters::{Format, FormatTitle}, search_author, search_book, search_sequence, search_translator,
                types::Page,
            },
            user_settings::get_user_or_default_lang_codes,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use self::{callback_data::SearchCallbackData, utils::get_query};

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
    Fut: std::future::Future<Output = Result<Page<T, P>, Box<dyn std::error::Error + Send + Sync>>>,
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
            bot
                .send_message(chat_id, "Ошибка! Попробуйте позже :(")
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
        items_page = match items_getter(
            query.clone(),
            items_page.pages,
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

    let formated_page = items_page.format(page, 4096);

    let keyboard = generic_get_pagination_keyboard(page, items_page.pages, search_data, true);

    bot
        .edit_message_text(chat_id, message_id, formated_page)
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
            .endpoint(|cq: CallbackQuery, callback_data: SearchCallbackData, bot: CacheMe<Throttle<Bot>>| async move {
                match callback_data {
                    SearchCallbackData::Book { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_book).await,
                    SearchCallbackData::Authors { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_author).await,
                    SearchCallbackData::Sequences { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_sequence).await,
                    SearchCallbackData::Translators { .. } => generic_search_pagination_handler(cq, bot, callback_data, search_translator).await,
                }
            })
    )
}
