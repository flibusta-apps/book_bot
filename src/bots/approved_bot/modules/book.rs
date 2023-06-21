use core::fmt::Debug;
use std::str::FromStr;

use moka::future::Cache;
use regex::Regex;
use smallvec::SmallVec;
use teloxide::{dispatching::UpdateFilterExt, dptree, prelude::*, adaptors::{Throttle, CacheMe}};

use crate::{bots::approved_bot::{
    services::{
        book_library::{
            formaters::{Format, FormatTitle}, get_author_books, get_sequence_books, get_translator_books,
            types::Page,
        },
        user_settings::get_user_or_default_lang_codes,
    },
    tools::filter_callback_query,
}, bots_manager::AppState};

use super::utils::{
    filter_command, generic_get_pagination_keyboard, CommandParse, GetPaginationCallbackData,
};

#[derive(Clone)]
pub enum BookCommand {
    Author { id: u32 },
    Translator { id: u32 },
    Sequence { id: u32 },
}

impl CommandParse<Self> for BookCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, strum::ParseError> {
        let re = Regex::new(r"^/(?P<an_type>a|t|s)_(?P<id>\d+)$").unwrap();

        let full_bot_name = format!("@{bot_name}");
        let after_replace = s.replace(&full_bot_name, "");

        let caps = re.captures(&after_replace);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let annotation_type = &caps["an_type"];
        let id: u32 = caps["id"].parse().unwrap();

        match annotation_type {
            "a" => Ok(BookCommand::Author { id }),
            "t" => Ok(BookCommand::Translator { id }),
            "s" => Ok(BookCommand::Sequence { id }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

#[derive(Clone)]
pub enum BookCallbackData {
    Author { id: u32, page: u32 },
    Translator { id: u32, page: u32 },
    Sequence { id: u32, page: u32 },
}

impl FromStr for BookCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^b(?P<an_type>a|t|s)_(?P<id>\d+)_(?P<page>\d+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let annotation_type = &caps["an_type"];
        let id = caps["id"].parse::<u32>().unwrap();
        let page = caps["page"].parse::<u32>().unwrap();

        match annotation_type {
            "a" => Ok(BookCallbackData::Author { id, page }),
            "t" => Ok(BookCallbackData::Translator { id, page }),
            "s" => Ok(BookCallbackData::Sequence { id, page }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

impl ToString for BookCallbackData {
    fn to_string(&self) -> String {
        match self {
            BookCallbackData::Author { id, page } => format!("ba_{id}_{page}"),
            BookCallbackData::Translator { id, page } => format!("bt_{id}_{page}"),
            BookCallbackData::Sequence { id, page } => format!("bs_{id}_{page}"),
        }
    }
}

impl GetPaginationCallbackData for BookCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        match self {
            BookCallbackData::Author { id, .. } => BookCallbackData::Author {
                id: *id,
                page: target_page,
            },
            BookCallbackData::Translator { id, .. } => BookCallbackData::Translator {
                id: *id,
                page: target_page,
            },
            BookCallbackData::Sequence { id, .. } => BookCallbackData::Sequence {
                id: *id,
                page: target_page,
            },
        }
        .to_string()
    }
}

async fn send_book_handler<T, P, Fut>(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    command: BookCommand,
    books_getter: fn(id: u32, page: u32, allowed_langs: SmallVec<[String; 3]>) -> Fut,
    user_langs_cache: Cache<UserId, SmallVec<[String; 3]>>,
) -> crate::bots::BotHandlerInternal
where
    T: Format + Clone + Debug,
    P: FormatTitle + Clone + Debug,
    Fut: std::future::Future<Output = Result<Page<T, P>, Box<dyn std::error::Error + Send + Sync>>>,
{
    let id = match command {
        BookCommand::Author { id } => id,
        BookCommand::Translator { id } => id,
        BookCommand::Sequence { id } => id,
    };

    let chat_id = message.chat.id;
    let user_id = message.from().map(|from| from.id);

    let user_id = match user_id {
        Some(v) => v,
        None => {
            return match bot
                .send_message(chat_id, "Повторите запрос сначала")
                .send()
                .await
            {
                Ok(_) => Ok(()),
                Err(err) => Err(Box::new(err)),
            }
        }
    };

    let allowed_langs = get_user_or_default_lang_codes(user_id, user_langs_cache).await;

    let items_page = match books_getter(id, 1, allowed_langs.clone()).await {
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
        bot.send_message(chat_id, "Книги не найдены!").send().await?;
        return Ok(());
    };

    let formated_page = items_page.format(1, 4096);

    let callback_data = match command {
        BookCommand::Author { id } => BookCallbackData::Author { id, page: 1 },
        BookCommand::Translator { id } => BookCallbackData::Translator { id, page: 1 },
        BookCommand::Sequence { id } => BookCallbackData::Sequence { id, page: 1 },
    };

    let keyboard = generic_get_pagination_keyboard(1, items_page.pages, callback_data, true);

    bot
        .send_message(chat_id, formated_page)
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
}

async fn send_pagination_book_handler<T, P, Fut>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    callback_data: BookCallbackData,
    books_getter: fn(id: u32, page: u32, allowed_langs: SmallVec<[String; 3]>) -> Fut,
    user_langs_cache: Cache<UserId, SmallVec<[String; 3]>>,
) -> crate::bots::BotHandlerInternal
where
    T: Format + Clone + Debug,
    P: FormatTitle + Clone + Debug,
    Fut: std::future::Future<Output = Result<Page<T, P>, Box<dyn std::error::Error + Send + Sync>>>,
{
    let (id, page) = match callback_data {
        BookCallbackData::Author { id, page } => (id, page),
        BookCallbackData::Translator { id, page } => (id, page),
        BookCallbackData::Sequence { id, page } => (id, page),
    };

    let chat_id = cq.message.as_ref().map(|message| message.chat.id);
    let user_id = cq.from.id;
    let message_id = cq.message.as_ref().map(|message| message.id);

    let (chat_id, message_id) = match (chat_id, message_id) {
        (Some(chat_id), Some(message_id)) => (chat_id, message_id),
        (Some(chat_id), None) => {
            bot.send_message(chat_id, "Повторите поиск сначала").send().await?;
            return Ok(());
        },
        _ => {
            return Ok(());
        }
    };

    let allowed_langs = get_user_or_default_lang_codes(user_id, user_langs_cache).await;

    let mut items_page = match books_getter(id, page, allowed_langs.clone()).await {
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

    if items_page.pages == 0 {
        bot.send_message(chat_id, "Книги не найдены!").send().await?;
        return Ok(());
    };

    if page > items_page.pages {
        items_page = match books_getter(id, items_page.pages, allowed_langs.clone()).await {
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

    let keyboard = generic_get_pagination_keyboard(page, items_page.pages, callback_data, true);

    bot
        .edit_message_text(chat_id, message_id, formated_page)
        .reply_markup(keyboard)
        .send()
        .await?;

    Ok(())
}

pub fn get_book_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message()
                .chain(filter_command::<BookCommand>())
                .endpoint(
                    |message: Message, bot: CacheMe<Throttle<Bot>>, command: BookCommand, app_state: AppState| async move {
                        match command {
                            BookCommand::Author { .. } => {
                                send_book_handler(
                                    message,
                                    bot,
                                    command,
                                    get_author_books,
                                    app_state.user_langs_cache
                                )
                                .await
                            }
                            BookCommand::Translator { .. } => {
                                send_book_handler(
                                    message,
                                    bot,
                                    command,
                                    get_translator_books,
                                    app_state.user_langs_cache
                                )
                                .await
                            }
                            BookCommand::Sequence { .. } => {
                                send_book_handler(
                                    message,
                                    bot,
                                    command,
                                    get_sequence_books,
                                    app_state.user_langs_cache,
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
                .endpoint(|cq: CallbackQuery, bot: CacheMe<Throttle<Bot>>, callback_data: BookCallbackData, app_state: AppState| async move {
                    match callback_data {
                        BookCallbackData::Author { .. } => send_pagination_book_handler(cq, bot, callback_data, get_author_books, app_state.user_langs_cache).await,
                        BookCallbackData::Translator { .. } => send_pagination_book_handler(cq, bot, callback_data,  get_translator_books, app_state.user_langs_cache).await,
                        BookCallbackData::Sequence { .. } => send_pagination_book_handler(cq, bot, callback_data,  get_sequence_books, app_state.user_langs_cache).await,
                    }
                }),
        )
}
