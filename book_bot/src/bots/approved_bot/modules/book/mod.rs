pub mod callback_data;
pub mod commands;

use book_bot_macros::log_handler;

use super::utils::constants::*;

use core::fmt::Debug;

use smartstring::alias::String as SmartString;

use smallvec::SmallVec;
use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::UpdateFilterExt,
    dptree,
    prelude::*,
    types::ReplyParameters,
};

use crate::bots::approved_bot::{
    services::{
        book_library::{
            formatters::{Format, FormatTitle},
            get_author_books, get_sequence_books, get_translator_books,
            types::Page,
        },
        user_settings::get_user_or_default_lang_codes,
    },
    tools::filter_callback_query,
};

use self::{callback_data::BookCallbackData, commands::BookCommand};

use super::utils::{
    filter_command::filter_command,
    pagination::{generic_get_pagination_keyboard, paginate, PaginationTexts},
    telegram_utils::{safe_send_message, safe_send_message_with_reply},
};

#[log_handler("book")]
async fn send_book_handler<T, P, Fut>(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    command: BookCommand,
    books_getter: fn(id: u32, page: u32, allowed_langs: SmallVec<[SmartString; 3]>) -> Fut,
) -> crate::bots::BotHandlerInternal
where
    T: Format + Clone + Debug,
    P: FormatTitle + Clone + Debug,
    Fut: std::future::Future<Output = anyhow::Result<Option<Page<T, P>>>>,
{
    let id = match command {
        BookCommand::Author { id } => id,
        BookCommand::Translator { id } => id,
        BookCommand::Sequence { id } => id,
    };

    let chat_id = message.chat.id;
    let user_id = match message.from.map(|from| from.id) {
        Some(v) => v,
        None => {
            return safe_send_message_with_reply(
                &bot,
                chat_id,
                REPEAT_REQUEST,
                ReplyParameters::new(message.id),
                None,
            )
            .await;
        }
    };

    let allowed_langs = get_user_or_default_lang_codes(user_id).await;

    let items_page = match books_getter(id, 1, allowed_langs).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message_with_reply(
                &bot,
                chat_id,
                NOT_FOUND,
                ReplyParameters::new(message.id),
                None,
            )
            .await?;
            return Ok(());
        }
        Err(err) => {
            safe_send_message_with_reply(
                &bot,
                chat_id,
                ERROR_TRY_LATER,
                ReplyParameters::new(message.id),
                None,
            )
            .await?;
            return Err(err);
        }
    };

    if items_page.pages == 0 {
        safe_send_message_with_reply(
            &bot,
            chat_id,
            BOOKS_NOT_FOUND,
            ReplyParameters::new(message.id),
            None,
        )
        .await?;
        return Ok(());
    };

    let formatted_page = items_page.format(1, TELEGRAM_MESSAGE_MAX_LENGTH);

    let callback_data = match command {
        BookCommand::Author { id } => BookCallbackData::Author { id, page: 1 },
        BookCommand::Translator { id } => BookCallbackData::Translator { id, page: 1 },
        BookCommand::Sequence { id } => BookCallbackData::Sequence { id, page: 1 },
    };

    let keyboard = generic_get_pagination_keyboard(1, items_page.pages, callback_data, true);

    safe_send_message_with_reply(
        &bot,
        chat_id,
        formatted_page,
        ReplyParameters::new(message.id),
        Some(keyboard),
    )
    .await?;

    Ok(())
}

#[log_handler("book")]
async fn send_pagination_book_handler<T, P, Fut>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    callback_data: BookCallbackData,
    books_getter: fn(id: u32, page: u32, allowed_langs: SmallVec<[SmartString; 3]>) -> Fut,
) -> crate::bots::BotHandlerInternal
where
    T: Format + Clone + Debug,
    P: FormatTitle + Clone + Debug,
    Fut: std::future::Future<Output = anyhow::Result<Option<Page<T, P>>>>,
{
    let (id, page) = match callback_data {
        BookCallbackData::Author { id, page } => (id, page),
        BookCallbackData::Translator { id, page } => (id, page),
        BookCallbackData::Sequence { id, page } => (id, page),
    };

    let chat_id = cq.message.as_ref().map(|message| message.chat().id);
    let user_id = cq.from.id;
    let message_id = cq.message.as_ref().map(|message| message.id());

    let (chat_id, message_id) = match (chat_id, message_id) {
        (Some(chat_id), Some(message_id)) => (chat_id, message_id),
        (Some(chat_id), None) => {
            safe_send_message(&bot, chat_id, REPEAT_SEARCH, None).await?;
            return Ok(());
        }
        _ => {
            return Ok(());
        }
    };

    let allowed_langs = get_user_or_default_lang_codes(user_id).await;

    paginate(
        &bot,
        chat_id,
        message_id,
        cq.message,
        page,
        "",
        |p| books_getter(id, p, allowed_langs.clone()),
        callback_data,
        PaginationTexts {
            not_found: NOT_FOUND,
            no_items: BOOKS_NOT_FOUND,
            error_try_later: Some(ERROR_TRY_LATER),
        },
    )
    .await
}

pub fn get_book_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message()
                .chain(filter_command::<BookCommand>())
                .endpoint(
                    |message: Message, bot: CacheMe<Throttle<Bot>>, command: BookCommand| async move {
                        match command {
                            BookCommand::Author { .. } => {
                                send_book_handler(
                                    message,
                                    bot,
                                    command,
                                    get_author_books,
                                )
                                .await
                            }
                            BookCommand::Translator { .. } => {
                                send_book_handler(
                                    message,
                                    bot,
                                    command,
                                    get_translator_books,
                                )
                                .await
                            }
                            BookCommand::Sequence { .. } => {
                                send_book_handler(
                                    message,
                                    bot,
                                    command,
                                    get_sequence_books,
                                )
                                .await
                            }
                        }
                    },
                ),
        )
        .branch(
            Update::filter_callback_query()
                .chain(filter_callback_query::<BookCallbackData>())
                .endpoint(|cq: CallbackQuery, bot: CacheMe<Throttle<Bot>>, callback_data: BookCallbackData| async move {
                    match callback_data {
                        BookCallbackData::Author { .. } => send_pagination_book_handler(cq, bot, callback_data, get_author_books).await,
                        BookCallbackData::Translator { .. } => send_pagination_book_handler(cq, bot, callback_data,  get_translator_books).await,
                        BookCallbackData::Sequence { .. } => send_pagination_book_handler(cq, bot, callback_data,  get_sequence_books).await,
                    }
                }),
        )
}
