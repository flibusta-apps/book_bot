use std::{str::FromStr, time::Duration};

use futures::TryStreamExt;
use moka::future::Cache;
use regex::Regex;
use strum_macros::EnumIter;
use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::UpdateFilterExt,
    dptree,
    prelude::*,
    types::*,
};
use tokio::time::sleep;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use url::Url;

use crate::{
    bots::{
        approved_bot::{
            services::{
                book_cache::{
                    download_file, get_cached_message,
                    types::{CachedMessage, DownloadFile},
                },
                book_library::{get_book, get_author_books_available_types, get_translator_books_available_types, get_sequence_books_available_types},
                donation_notificatioins::send_donation_notification, user_settings::get_user_or_default_lang_codes, batch_downloader::{TaskObjectType, CreateTaskData},
                batch_downloader::{create_task, get_task, TaskStatus}

            },
            tools::filter_callback_query,
        },
        BotHandlerInternal,
    },
    bots_manager::{AppState, BotCache},
};

use super::utils::{filter_command, CommandParse};

#[derive(Clone)]
pub struct StartDownloadCommand {
    pub id: u32,
}

impl ToString for StartDownloadCommand {
    fn to_string(&self) -> String {
        let StartDownloadCommand { id } = self;
        format!("/d_{id}")
    }
}

impl CommandParse<Self> for StartDownloadCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, strum::ParseError> {
        let re = Regex::new(r"^/d_(?P<book_id>\d+)$").unwrap();

        let full_bot_name = format!("@{bot_name}");
        let after_replace = s.replace(&full_bot_name, "");

        let caps = re.captures(&after_replace);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let book_id: u32 = caps["book_id"].parse().unwrap();

        Ok(StartDownloadCommand { id: book_id })
    }
}

#[derive(Clone, EnumIter)]
pub enum DownloadQueryData {
    DownloadData { book_id: u32, file_type: String },
}

impl ToString for DownloadQueryData {
    fn to_string(&self) -> String {
        match self {
            DownloadQueryData::DownloadData { book_id, file_type } => {
                format!("d_{book_id}_{file_type}")
            }
        }
    }
}

impl FromStr for DownloadQueryData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^d_(?P<book_id>\d+)_(?P<file_type>\w+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let book_id: u32 = caps["book_id"].parse().unwrap();
        let file_type: String = caps["file_type"].to_string();

        Ok(DownloadQueryData::DownloadData { book_id, file_type })
    }
}

#[derive(Clone, EnumIter)]
pub enum DownloadArchiveCommand {
    Sequence { id: u32},
    Author { id: u32 },
    Translator { id: u32 }
}

impl ToString for DownloadArchiveCommand {
    fn to_string(&self) -> String {
        match self {
            DownloadArchiveCommand::Sequence { id } => format!("/da_s_{id}"),
            DownloadArchiveCommand::Author { id } => format!("/da_a_{id}"),
            DownloadArchiveCommand::Translator { id } => format!("/da_t_{id}"),
        }
    }
}

impl CommandParse<Self> for DownloadArchiveCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, strum::ParseError> {
        let re = Regex::new(r"^/da_(?P<type>[s|a|t])_(?P<id>\d+)$").unwrap();

        let full_bot_name = format!("@{bot_name}");
        let after_replace = s.replace(&full_bot_name, "");

        let caps = re.captures(&after_replace);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let obj_id: u32 = caps["id"].parse().unwrap();

        match &caps["type"] {
            "s" => Ok(DownloadArchiveCommand::Sequence { id: obj_id }),
            "a" => Ok(DownloadArchiveCommand::Author { id: obj_id }),
            "t" => Ok(DownloadArchiveCommand::Translator { id: obj_id }),
            _ => Err(strum::ParseError::VariantNotFound)
        }
    }
}

#[derive(Clone, EnumIter)]
pub enum DownloadArchiveQueryData {
    Sequence { id: u32, file_type: String },
    Author { id: u32, file_type: String },
    Translator { id: u32, file_type: String }
}

impl ToString for DownloadArchiveQueryData {
    fn to_string(&self) -> String {
        match self {
            DownloadArchiveQueryData::Sequence { id, file_type } => format!("da_s_{id}_{file_type}"),
            DownloadArchiveQueryData::Author { id, file_type } => format!("da_a_{id}_{file_type}"),
            DownloadArchiveQueryData::Translator { id, file_type } => format!("da_t_{id}_{file_type}"),
        }
    }
}

impl FromStr for DownloadArchiveQueryData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^da_(?P<obj_type>[s|a|t])_(?P<id>\d+)_(?P<file_type>\w+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let id: u32 = caps["book_id"].parse().unwrap();
        let file_type: String = caps["file_type"].to_string();

        Ok(
            match caps["obj_type"].to_string().as_str() {
                "s" => DownloadArchiveQueryData::Sequence { id, file_type },
                "a" => DownloadArchiveQueryData::Author { id, file_type },
                "t" => DownloadArchiveQueryData::Translator { id, file_type },
                _ => return Err(strum::ParseError::VariantNotFound)
            }
        )
    }
}

async fn _send_cached(
    message: &Message,
    bot: &CacheMe<Throttle<Bot>>,
    cached_message: CachedMessage,
) -> BotHandlerInternal {
    match bot
        .copy_message(
            message.chat.id,
            Recipient::Id(ChatId(cached_message.data.chat_id)),
            MessageId(cached_message.data.message_id),
        )
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

async fn send_cached_message(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadQueryData,
    donation_notification_cache: Cache<ChatId, bool>,
    need_delete_message: bool,
) -> BotHandlerInternal {
    if let Ok(v) = get_cached_message(&download_data).await {
        if _send_cached(&message, &bot, v).await.is_ok() {
            if need_delete_message {
                bot.delete_message(message.chat.id, message.id).await?;
            }

            send_donation_notification(bot.clone(), message, donation_notification_cache).await?;

            return Ok(());
        }
    };

    send_with_download_from_channel(message, bot, download_data, donation_notification_cache, need_delete_message)
        .await?;

    Ok(())
}

async fn _send_downloaded_file(
    message: &Message,
    bot: CacheMe<Throttle<Bot>>,
    downloaded_data: DownloadFile,
    donation_notification_cache: Cache<ChatId, bool>,
) -> BotHandlerInternal {
    let DownloadFile {
        response,
        filename,
        caption,
    } = downloaded_data;

    let data = response
        .bytes_stream()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        .into_async_read()
        .compat();

    let document: InputFile = InputFile::read(data).file_name(filename);

    bot.send_document(message.chat.id, document)
        .caption(caption)
        .send()
        .await?;

    send_donation_notification(bot, message.clone(), donation_notification_cache).await?;

    Ok(())
}

async fn send_with_download_from_channel(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadQueryData,
    donation_notification_cache: Cache<ChatId, bool>,
    need_delete_message: bool,
) -> BotHandlerInternal {
    match download_file(&download_data).await {
        Ok(v) => {
            _send_downloaded_file(&message, bot.clone(), v, donation_notification_cache).await?;

            if need_delete_message {
                bot.delete_message(message.chat.id, message.id).await?;
            }

            Ok(())
        },
        Err(err) => Err(err),
    }
}

async fn download_handler(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    cache: BotCache,
    download_data: DownloadQueryData,
    donation_notification_cache: Cache<ChatId, bool>,
    need_delete_message: bool,
) -> BotHandlerInternal {
    match cache {
        BotCache::Original => {
            send_cached_message(
                message,
                bot,
                download_data,
                donation_notification_cache,
                need_delete_message,
            )
            .await
        }
        BotCache::NoCache => {
            send_with_download_from_channel(
                message,
                bot,
                download_data,
                donation_notification_cache,
                need_delete_message,
            )
            .await
        }
    }
}

async fn get_download_keyboard_handler(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    download_data: StartDownloadCommand,
) -> BotHandlerInternal {
    let book = match get_book(download_data.id).await {
        Ok(v) => v,
        Err(err) => {
            bot.send_message(message.chat.id, "Ошибка! Попробуйте позже :(")
                .send()
                .await?;

            return Err(err);
        }
    };

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: book
            .available_types
            .into_iter()
            .map(|item| -> Vec<InlineKeyboardButton> {
                vec![InlineKeyboardButton {
                    text: {
                        format!("📥 {item}")
                    },
                    kind: InlineKeyboardButtonKind::CallbackData(
                        (DownloadQueryData::DownloadData {
                            book_id: book.id,
                            file_type: item,
                        })
                        .to_string(),
                    ),
                }]
            })
            .collect(),
    };

    bot.send_message(message.chat.id, "Выбери формат:")
        .reply_markup(keyboard)
        .reply_to_message_id(message.id)
        .send()
        .await?;

    Ok(())
}

async fn get_download_archive_keyboard_handler(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    command: DownloadArchiveCommand,
    app_state: AppState,
) -> BotHandlerInternal {
    let user_langs_cache = app_state.user_langs_cache;

    let allowed_langs = get_user_or_default_lang_codes(
        message.from().unwrap().id,
        user_langs_cache
    ).await;

    let available_types = match command {
        DownloadArchiveCommand::Sequence { id } => get_sequence_books_available_types(id, allowed_langs).await,
        DownloadArchiveCommand::Author { id } => get_author_books_available_types(id, allowed_langs).await,
        DownloadArchiveCommand::Translator { id } => get_translator_books_available_types(id, allowed_langs).await,
    };

    let available_types = match available_types {
        Ok(v) => v,
        Err(err) => return Err(err),
    };

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard:
            available_types.iter()
            .filter(|file_type| !file_type.contains("zip"))
            .map(|file_type| {
                let callback_data: String = match command {
                    DownloadArchiveCommand::Sequence { id } => DownloadArchiveQueryData::Sequence {
                        id, file_type: file_type.to_string()
                    }.to_string(),
                    DownloadArchiveCommand::Author { id } => DownloadArchiveQueryData::Author {
                        id, file_type: file_type.to_string()
                    }.to_string(),
                    DownloadArchiveCommand::Translator { id } => DownloadArchiveQueryData::Translator {
                        id, file_type: file_type.to_string()
                    }.to_string(),
                };

                vec![InlineKeyboardButton {
                    text: file_type.to_string(),
                    kind: InlineKeyboardButtonKind::CallbackData(callback_data)
                }]
            }).collect()
    };

    bot
        .send_message(message.chat.id, "Функция в разработке")
        .reply_markup(keyboard)
        .reply_to_message_id(message.id)
        .await?;

    Ok(())
}

async fn download_archive(
    cq: CallbackQuery,
    download_archive_query_data: DownloadArchiveQueryData,
    bot: CacheMe<Throttle<Bot>>,
    app_state: AppState
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(
        cq.from.id,
        app_state.user_langs_cache
    ).await;

    let (id, file_type, task_type) = match download_archive_query_data {
        DownloadArchiveQueryData::Sequence { id, file_type } => (id, file_type, TaskObjectType::Sequence),
        DownloadArchiveQueryData::Author { id, file_type } => (id, file_type, TaskObjectType::Author),
        DownloadArchiveQueryData::Translator { id, file_type } => (id, file_type, TaskObjectType::Translator),
    };

    let message = cq.message.unwrap();

    let task = create_task(CreateTaskData {
        object_id: id,
        object_type: task_type,
        file_format: file_type,
        allowed_langs
    }).await;

    let mut task = match task {
        Ok(v) => v,
        Err(err) => {
            bot
                .edit_message_text(message.chat.id, message.id, "Ошибка! Попробуйте позже :(")
                .reply_markup(InlineKeyboardMarkup {
                    inline_keyboard: vec![],
                })
                .send()
                .await?;

            return Err(err);
        },
    };

    bot
        .edit_message_text(message.chat.id, message.id, "Подготовка архива...")
        .reply_markup(InlineKeyboardMarkup {
            inline_keyboard: vec![],
        })
        .send()
        .await?;

    while task.status != TaskStatus::Complete {
        task = match get_task(task.id).await {
            Ok(v) => v,
            Err(err) => {
                bot
                .edit_message_text(message.chat.id, message.id, "Ошибка! Попробуйте позже :(")
                .reply_markup(InlineKeyboardMarkup {
                    inline_keyboard: vec![],
                })
                .send()
                .await?;

                return Err(err);
            },
        };

        sleep(Duration::from_secs(5)).await;
    }

    bot
        .send_document(
            message.chat.id,
            InputFile::url(
                Url::from_str(&task.result_link.unwrap()
            ).unwrap())
        )
        .send()
        .await?;

    Ok(())
}

pub fn get_download_hander() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message()
                .chain(filter_command::<StartDownloadCommand>())
                .endpoint(get_download_keyboard_handler),
        )
        .branch(
            Update::filter_callback_query()
                .chain(filter_callback_query::<DownloadQueryData>())
                .endpoint(
                    |cq: CallbackQuery,
                     download_query_data: DownloadQueryData,
                     bot: CacheMe<Throttle<Bot>>,
                     cache: BotCache,
                     app_state: AppState| async move {
                        download_handler(
                            cq.message.unwrap(),
                            bot,
                            cache,
                            download_query_data,
                            app_state.chat_donation_notifications_cache,
                            true,
                        )
                        .await
                    },
                ),
        )
        .branch(
            Update::filter_message()
                .chain(filter_command::<DownloadArchiveCommand>())
                .endpoint(get_download_archive_keyboard_handler)
        )
        .branch(
            Update::filter_callback_query()
            .chain(filter_callback_query::<DownloadArchiveQueryData>())
            .endpoint(download_archive)
        )
}
