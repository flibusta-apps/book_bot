use std::{str::FromStr, time::Duration};

use futures::TryStreamExt;
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

use crate::{
    bots::{
        approved_bot::{
            services::{
                book_cache::{
                    download_file, get_cached_message,
                    types::{CachedMessage, DownloadFile}, download_file_by_link,
                },
                book_library::{get_book, get_author_books_available_types, get_translator_books_available_types, get_sequence_books_available_types},
                donation_notificatioins::send_donation_notification, user_settings::get_user_or_default_lang_codes, batch_downloader::{TaskObjectType, CreateTaskData},
                batch_downloader::{create_task, get_task, TaskStatus}

            },
            tools::filter_callback_query,
        },
        BotHandlerInternal,
    },
    bots_manager::BotCache,
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

        let id: u32 = caps["id"].parse().unwrap();
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


#[derive(Clone)]
pub struct CheckArchiveStatus {
    pub task_id: String
}

impl ToString for CheckArchiveStatus {
    fn to_string(&self) -> String {
    format!("check_da_{}", self.task_id)
    }
}

impl FromStr for CheckArchiveStatus {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^check_da_(?P<task_id>\w+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let task_id: String = caps["task_id"].parse().unwrap();

        Ok(CheckArchiveStatus { task_id })
    }
}


fn get_check_keyboard(task_id: String) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    (CheckArchiveStatus { task_id }).to_string(),
                ),
                text: String::from("Обновить статус"),
            }],
        ],
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
    need_delete_message: bool,
) -> BotHandlerInternal {
    if let Ok(v) = get_cached_message(&download_data).await {
        if _send_cached(&message, &bot, v).await.is_ok() {
            if need_delete_message {
                bot.delete_message(message.chat.id, message.id).await?;
            }

            send_donation_notification(bot.clone(), message).await?;

            return Ok(());
        }
    };

    send_with_download_from_channel(message, bot, download_data, need_delete_message)
        .await?;

    Ok(())
}

async fn _send_downloaded_file(
    message: &Message,
    bot: CacheMe<Throttle<Bot>>,
    downloaded_data: DownloadFile,
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

    send_donation_notification(bot, message.clone()).await?;

    Ok(())
}

async fn send_with_download_from_channel(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadQueryData,
    need_delete_message: bool,
) -> BotHandlerInternal {
    match download_file(&download_data).await {
        Ok(v) => {
            _send_downloaded_file(&message, bot.clone(), v).await?;

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
    need_delete_message: bool,
) -> BotHandlerInternal {
    match cache {
        BotCache::Original => {
            send_cached_message(
                message,
                bot,
                download_data,
                need_delete_message,
            )
            .await
        }
        BotCache::NoCache => {
            send_with_download_from_channel(
                message,
                bot,
                download_data,
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
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(
        message.from().unwrap().id,
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
        .send_message(message.chat.id, "Выбери формат:")
        .reply_markup(keyboard)
        .reply_to_message_id(message.id)
        .await?;

    Ok(())
}

async fn send_error_message(bot: CacheMe<Throttle<Bot>>, chat_id: ChatId, message_id: MessageId) {
    let _ = bot
        .edit_message_text(chat_id, message_id, "Ошибка! Попробуйте позже :(")
        .reply_markup(InlineKeyboardMarkup {
            inline_keyboard: vec![],
        })
        .send()
        .await;
}

async fn wait_archive(
    bot: CacheMe<Throttle<Bot>>,
    task_id: String,
    message: Message,
) -> BotHandlerInternal {
    let task = loop {
        let task = match get_task(task_id.clone()).await {
            Ok(v) => v,
            Err(err) => {
                send_error_message(bot, message.chat.id, message.id).await;
                log::error!("{:?}", err);
                return Err(err);
            },
        };

        if task.status != TaskStatus::InProgress {
            break task;
        }

        bot
            .edit_message_text(
                message.chat.id,
                message.id,
                format!("Статус: \n ⏳ {}", task.status_description)
            )
            .reply_markup(get_check_keyboard(task.id))
            .send()
            .await?;

        sleep(Duration::from_secs(5)).await;
    };

    if task.status != TaskStatus::Complete {
        send_error_message(bot, message.chat.id, message.id).await;
        return Ok(());
    }

    let downloaded_data = match download_file_by_link(
        task.result_filename.unwrap(),
        task.result_link.clone().unwrap()
    ).await {
        Ok(v) => v,
        Err(err) => {
            send_error_message(bot, message.chat.id, message.id).await;
            log::error!("{:?}", err);
            return Err(err);
        },
    };

    match _send_downloaded_file(
        &message,
        bot.clone(),
        downloaded_data,
    ).await {
        Ok(_) => (),
        Err(err) => {
            let _ = bot
                .edit_message_text(
                    message.chat.id,
                    message.id,
                    format!(
                        "Файл не может быть загружен в чат! \nВы можете скачать его <a href=\"{}\">по ссылке</a> (работает 24 часа)",
                        task.result_link.unwrap()
                    )
                )
                .parse_mode(ParseMode::Html)
                .reply_markup(InlineKeyboardMarkup {
                    inline_keyboard: vec![],
                })
                .send()
                .await;
            log::error!("{:?}", err);
            return Err(err);
        },
    }

    bot
        .delete_message(message.chat.id, message.id)
        .await?;

    Ok(())
}


async fn download_archive(
    cq: CallbackQuery,
    download_archive_query_data: DownloadArchiveQueryData,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(
        cq.from.id,
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
        allowed_langs,
    }).await;

    let task = match task {
        Ok(v) => v,
        Err(err) => {
            send_error_message(bot, message.chat.id, message.id).await;
            log::error!("{:?}", err);
            return Err(err);
        },
    };

    bot
        .edit_message_text(message.chat.id, message.id, "⏳ Подготовка архива...")
        .reply_markup(get_check_keyboard(task.id.clone()))
        .send()
        .await?;

    let _ = wait_archive(bot, task.id, message).await;

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
                     cache: BotCache| async move {
                        download_handler(
                            cq.message.unwrap(),
                            bot,
                            cache,
                            download_query_data,
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
        .branch(
            Update::filter_callback_query()
            .chain(filter_callback_query::<CheckArchiveStatus>())
            .endpoint(|cq: CallbackQuery, status: CheckArchiveStatus, bot: CacheMe<Throttle<Bot>>| async move {
                wait_archive(bot, status.task_id, cq.message.unwrap()).await
            })
        )
}
