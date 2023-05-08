use strum_macros::{Display, EnumIter};
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
    utils::command::BotCommands, adaptors::{Throttle, CacheMe},
};

use crate::bots::{
    approved_bot::{
        services::{
            book_library::{self, formaters::Format},
            user_settings::get_user_or_default_lang_codes,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum RandomCommand {
    Random,
}

#[derive(Clone, Display, EnumIter)]
#[strum(serialize_all = "snake_case")]
enum RandomCallbackData {
    RandomBook,
    RandomAuthor,
    RandomSequence,
    RandomBookByGenreRequest,
    Genres { index: u32 },
    RandomBookByGenre { id: u32 },
}

impl std::str::FromStr for RandomCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.to_string();

        for callback_data in <RandomCallbackData as strum::IntoEnumIterator>::iter() {
            match callback_data {
                RandomCallbackData::Genres { .. }
                | RandomCallbackData::RandomBookByGenre { .. } => {
                    let callback_prefix = callback_data.to_string();

                    if value.starts_with(&callback_prefix) {
                        let data: u32 = value
                            .strip_prefix(&format!("{}_", &callback_prefix).to_string())
                            .unwrap()
                            .parse()
                            .unwrap();

                        match callback_data {
                            RandomCallbackData::Genres { .. } => {
                                return Ok(RandomCallbackData::Genres { index: data })
                            }
                            RandomCallbackData::RandomBookByGenre { .. } => {
                                return Ok(RandomCallbackData::RandomBookByGenre { id: data })
                            }
                            _ => (),
                        }
                    }
                }
                _ => {
                    if value == callback_data.to_string() {
                        return Ok(callback_data);
                    }
                }
            }
        }

        Err(strum::ParseError::VariantNotFound)
    }
}

async fn random_handler(message: Message, bot: CacheMe<Throttle<Bot>>) -> crate::bots::BotHandlerInternal {
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

    let res = bot
        .send_message(message.chat.id, MESSAGE_TEXT)
        .reply_to_message_id(message.id)
        .reply_markup(keyboard)
        .send()
        .await;

    match res {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

async fn get_random_item_handler_internal<T>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    item: Result<T, Box<dyn std::error::Error + Send + Sync>>,
) -> BotHandlerInternal
where
    T: Format,
{
    match item {
        Ok(item) => {
            let item_message = item.format();

            let send_item_handler = tokio::spawn(
                bot.send_message(cq.from.id, item_message)
                    .reply_markup(InlineKeyboardMarkup {
                        inline_keyboard: vec![vec![InlineKeyboardButton {
                            kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                                cq.data.unwrap(),
                            ),
                            text: String::from("Повторить?"),
                        }]],
                    })
                    .send(),
            );

            cq.message.map(|message| async move {
                bot.edit_message_reply_markup(message.chat.id, message.id)
                    .reply_markup(InlineKeyboardMarkup {
                        inline_keyboard: vec![],
                    })
                    .send()
                    .await
            });

            match send_item_handler.await {
                Ok(_) => Ok(()),
                Err(err) => Err(Box::new(err)),
            }
        }
        Err(err) => {
            match bot
                .send_message(cq.from.id, "Ошибка! Попробуйте позже :(")
                .send()
                .await
            {
                Ok(_) => (),
                Err(int_error) => return Err(Box::new(int_error)),
            }

            Err(err)
        }
    }
}

async fn get_random_item_handler<T, Fut>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    item_getter: fn(allowed_langs: Vec<String>) -> Fut,
) -> BotHandlerInternal
where
    T: Format,
    Fut: std::future::Future<Output = Result<T, Box<dyn std::error::Error + Send + Sync>>>,
{
    let allowed_langs = get_user_or_default_lang_codes(cq.from.id).await;

    let item = item_getter(allowed_langs).await;

    get_random_item_handler_internal(cq, bot, item).await
}

async fn get_genre_metas_handler(cq: CallbackQuery, bot: CacheMe<Throttle<Bot>>) -> BotHandlerInternal {
    let genre_metas = match book_library::get_genre_metas().await {
        Ok(v) => v,
        Err(err) => return Err(err),
    };

    match cq.message {
        Some(message) => {
            let keyboard = InlineKeyboardMarkup {
                inline_keyboard: genre_metas
                    .clone()
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

            match bot
                .edit_message_reply_markup(message.chat.id, message.id)
                .reply_markup(keyboard)
                .send()
                .await
            {
                Ok(_) => Ok(()),
                Err(err) => Err(Box::new(err)),
            }
        }
        None => {
            match bot
                .send_message(cq.from.id, "Ошибка! Начните заново :(")
                .send()
                .await
            {
                Ok(_) => Ok(()),
                Err(err) => Err(Box::new(err)),
            }
        }
    }
}

async fn get_genres_by_meta_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    genre_index: u32,
) -> BotHandlerInternal {
    let genre_metas = match book_library::get_genre_metas().await {
        Ok(v) => v,
        Err(err) => return Err(err),
    };

    let meta = match genre_metas.get(genre_index as usize) {
        Some(v) => v,
        None => {
            return match bot
                .send_message(cq.from.id, "Ошибка! Попробуйте позже :(")
                .send()
                .await
            {
                Ok(_) => Ok(()),
                Err(err) => Err(Box::new(err)),
            }
        }
    };

    let genres = match book_library::get_genres(meta.to_string()).await {
        Ok(v) => v.items,
        Err(err) => return Err(err),
    };

    let mut buttons: Vec<Vec<InlineKeyboardButton>> = genres
        .clone()
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

    match cq.message {
        Some(message) => {
            match bot
                .edit_message_reply_markup(message.chat.id, message.id)
                .reply_markup(keyboard)
                .send()
                .await
            {
                Ok(_) => Ok(()),
                Err(err) => Err(Box::new(err)),
            }
        }
        None => {
            match bot
                .send_message(cq.from.id, "Ошибка! Начните заново :(")
                .send()
                .await
            {
                Ok(_) => Ok(()),
                Err(err) => Err(Box::new(err)),
            }
        }
    }
}

async fn get_random_book_by_genre(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    genre_id: u32,
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(cq.from.id).await;

    let item = book_library::get_random_book_by_genre(allowed_langs, Some(genre_id)).await;

    get_random_item_handler_internal(cq, bot, item).await
}

pub fn get_random_hander() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message()
                .branch(
                    dptree::entry()
                        .filter_command::<RandomCommand>()
                        .endpoint(|message, command, bot| async {
                            match command {
                                RandomCommand::Random => random_handler(message, bot).await,
                            }
                        })
                )
        )
        .branch(
            Update::filter_callback_query()
                .chain(filter_callback_query::<RandomCallbackData>())
                .endpoint(|cq: CallbackQuery, callback_data: RandomCallbackData, bot: CacheMe<Throttle<Bot>>| async move {
                    match callback_data {
                        RandomCallbackData::RandomBook => get_random_item_handler(cq, bot, book_library::get_random_book).await,
                        RandomCallbackData::RandomAuthor => get_random_item_handler(cq, bot, book_library::get_random_author).await,
                        RandomCallbackData::RandomSequence => get_random_item_handler(cq, bot, book_library::get_random_sequence).await,
                        RandomCallbackData::RandomBookByGenreRequest => get_genre_metas_handler(cq, bot).await,
                        RandomCallbackData::Genres { index } => get_genres_by_meta_handler(cq, bot, index).await,
                        RandomCallbackData::RandomBookByGenre { id } => get_random_book_by_genre(cq, bot, id).await,
                    }
                })
        )
}
