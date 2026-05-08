pub mod callback_data;
pub mod commands;

use book_bot_macros::log_handler;

use smallvec::SmallVec;
use smartstring::alias::String as SmartString;
use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, ReplyParameters},
};

use crate::bots::{
    approved_bot::{
        modules::random::callback_data::RandomCallbackData,
        modules::utils::telegram_utils::{
            safe_edit_message_reply_markup, safe_send_message, safe_send_message_with_reply,
        },
        services::{
            book_library::{self, formatters::Format},
            user_settings::get_user_or_default_lang_codes,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use self::commands::RandomCommand;

#[log_handler("random")]
async fn random_handler(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
) -> crate::bots::BotHandlerInternal {
    const MESSAGE_TEXT: &str = "Что хотим получить?";

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    RandomCallbackData::RandomBook.to_string(),
                ),
                text: String::from("Книгу"),
            }],
            vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    RandomCallbackData::RandomBookByGenreRequest.to_string(),
                ),
                text: String::from("Книгу по жанру"),
            }],
            vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    RandomCallbackData::RandomAuthor.to_string(),
                ),
                text: String::from("Автора"),
            }],
            vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    RandomCallbackData::RandomSequence.to_string(),
                ),
                text: String::from("Серию"),
            }],
        ],
    };

    safe_send_message_with_reply(
        &bot,
        message.chat.id,
        MESSAGE_TEXT,
        ReplyParameters::new(message.id),
        Some(keyboard),
    )
    .await?;

    Ok(())
}

async fn get_random_item_handler_internal<T>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    item: anyhow::Result<Option<T>>,
) -> BotHandlerInternal
where
    T: Format,
{
    let item = match item {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, cq.from.id.into(), "Не найдено :(", None).await?;
            return Ok(());
        }
        Err(err) => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Попробуйте позже :(", None).await?;
            return Err(err);
        }
    };

    let item_message = item.format(4096).result;

    safe_send_message(
        &bot,
        cq.from.id.into(),
        item_message,
        Some(InlineKeyboardMarkup {
            inline_keyboard: vec![vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(cq.data.unwrap()),
                text: String::from("Повторить?"),
            }]],
        }),
    )
    .await?;

    match cq.message {
        Some(message) => {
            safe_edit_message_reply_markup(
                &bot,
                message.chat().id,
                message.id(),
                InlineKeyboardMarkup {
                    inline_keyboard: vec![],
                },
            )
            .await?;
            Ok(())
        }
        None => Ok(()),
    }
}

#[log_handler("random")]
async fn get_random_item_handler<T, Fut>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    item_getter: fn(allowed_langs: SmallVec<[SmartString; 3]>) -> Fut,
) -> BotHandlerInternal
where
    T: Format,
    Fut: std::future::Future<Output = anyhow::Result<Option<T>>>,
{
    let allowed_langs = get_user_or_default_lang_codes(cq.from.id).await;

    let item = item_getter(allowed_langs).await;

    get_random_item_handler_internal(cq, bot, item).await
}

#[log_handler("random")]
async fn get_genre_metas_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let genre_metas = match book_library::get_genre_metas().await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, cq.from.id.into(), "Не найдено :(", None).await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let message = match cq.message {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Начните заново :(", None).await?;
            return Ok(());
        }
    };

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: genre_metas
            .into_iter()
            .enumerate()
            .map(|(index, genre_meta)| {
                vec![InlineKeyboardButton {
                    kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(format!(
                        "{}_{index}",
                        RandomCallbackData::Genres {
                            index: index as u32
                        }
                    )),
                    text: genre_meta,
                }]
            })
            .collect(),
    };

    safe_edit_message_reply_markup(&bot, message.chat().id, message.id(), keyboard).await?;

    Ok(())
}

#[log_handler("random")]
async fn get_genres_by_meta_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    genre_index: u32,
) -> BotHandlerInternal {
    let genre_metas = match book_library::get_genre_metas().await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, cq.from.id.into(), "Не найдено :(", None).await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let meta = match genre_metas.get(genre_index as usize) {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Попробуйте позже :(", None).await?;

            return Ok(());
        }
    };

    let genres_page = match book_library::get_genres(meta.into()).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, cq.from.id.into(), "Не найдено :(", None).await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let mut buttons: Vec<Vec<InlineKeyboardButton>> = genres_page
        .items
        .into_iter()
        .map(|genre| {
            vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(format!(
                    "{}_{}",
                    RandomCallbackData::RandomBookByGenre { id: genre.id },
                    genre.id
                )),
                text: genre.description,
            }]
        })
        .collect();

    buttons.push(vec![InlineKeyboardButton {
        kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
            RandomCallbackData::RandomBookByGenreRequest.to_string(),
        ),
        text: "< Назад >".to_string(),
    }]);

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: buttons,
    };

    let message = match cq.message {
        Some(message) => message,
        None => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Начните заново :(", None).await?;

            return Ok(());
        }
    };

    safe_edit_message_reply_markup(&bot, message.chat().id, message.id(), keyboard).await?;

    Ok(())
}

#[log_handler("random")]
async fn get_random_book_by_genre(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    genre_id: u32,
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(cq.from.id).await;

    let item = book_library::get_random_book_by_genre(allowed_langs, Some(genre_id)).await;

    get_random_item_handler_internal(cq, bot, item).await
}

pub fn get_random_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(Update::filter_message().branch(
            dptree::entry().filter_command::<RandomCommand>().endpoint(
                |message, command, bot| async {
                    match command {
                        RandomCommand::Random => random_handler(message, bot).await,
                    }
                },
            ),
        ))
        .branch(
            Update::filter_callback_query()
                .chain(filter_callback_query::<RandomCallbackData>())
                .endpoint(
                    |cq: CallbackQuery,
                     callback_data: RandomCallbackData,
                     bot: CacheMe<Throttle<Bot>>| async move {
                        match callback_data {
                            RandomCallbackData::RandomBook => {
                                get_random_item_handler(cq, bot, book_library::get_random_book)
                                    .await
                            }
                            RandomCallbackData::RandomAuthor => {
                                get_random_item_handler(cq, bot, book_library::get_random_author)
                                    .await
                            }
                            RandomCallbackData::RandomSequence => {
                                get_random_item_handler(cq, bot, book_library::get_random_sequence)
                                    .await
                            }
                            RandomCallbackData::RandomBookByGenreRequest => {
                                get_genre_metas_handler(cq, bot).await
                            }
                            RandomCallbackData::Genres { index } => {
                                get_genres_by_meta_handler(cq, bot, index).await
                            }
                            RandomCallbackData::RandomBookByGenre { id } => {
                                get_random_book_by_genre(cq, bot, id).await
                            }
                        }
                    },
                ),
        )
}
