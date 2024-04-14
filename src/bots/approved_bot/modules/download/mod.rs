pub mod callback_data;
pub mod commands;

use std::time::Duration;

use chrono::Utc;
use futures::TryStreamExt;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::UpdateFilterExt,
    dptree,
    prelude::*,
    types::*,
};
use tokio::time::{self};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::log;

use crate::{
    bots::{
        approved_bot::{
            modules::download::callback_data::DownloadArchiveQueryData,
            services::{
                batch_downloader::{create_task, get_task, Task, TaskStatus},
                batch_downloader::{CreateTaskData, TaskObjectType},
                book_cache::{
                    download_file, download_file_by_link, get_cached_message, get_download_link,
                    types::{CachedMessage, DownloadFile},
                },
                book_library::{
                    get_author_books_available_types, get_book, get_sequence_books_available_types,
                    get_translator_books_available_types,
                },
                donation_notifications::send_donation_notification,
                user_settings::get_user_or_default_lang_codes,
            },
            tools::filter_callback_query,
        },
        BotHandlerInternal,
    },
    bots_manager::BotCache,
};

use self::{
    callback_data::{CheckArchiveStatus, DownloadQueryData},
    commands::{DownloadArchiveCommand, StartDownloadCommand},
};

use super::utils::filter_command::filter_command;

fn get_check_keyboard(task_id: String) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton {
            kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                (CheckArchiveStatus { task_id }).to_string(),
            ),
            text: String::from("Обновить статус"),
        }]],
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
            Recipient::Id(ChatId(cached_message.chat_id)),
            MessageId(cached_message.message_id),
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

            match send_donation_notification(bot.clone(), message).await {
                Ok(_) => (),
                Err(err) => log::error!("{:?}", err),
            }

            return Ok(());
        }
    };

    send_with_download_from_channel(message, bot, download_data, need_delete_message).await?;

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

    match send_donation_notification(bot, message.clone()).await {
        Ok(_) => (),
        Err(err) => log::error!("{:?}", err),
    };

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
            if _send_downloaded_file(&message, bot.clone(), v)
                .await
                .is_err()
            {
                send_download_link(message.clone(), bot.clone(), download_data).await?;
                return Ok(());
            };

            if need_delete_message {
                bot.delete_message(message.chat.id, message.id).await?;
            }

            Ok(())
        }
        Err(err) => Err(err),
    }
}

async fn send_download_link(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadQueryData,
) -> BotHandlerInternal {
    let link_data = match get_download_link(&download_data).await {
        Ok(v) => v,
        Err(err) => {
            log::error!("{:?}", err);
            return Err(err);
        }
    };

    bot.edit_message_text(
        message.chat.id,
        message.id,
        format!(
            "Файл не может быть загружен в чат! \n \
                Вы можете скачать его <a href=\"{}\">по ссылке</a> (работает 3 часа)",
            link_data.link
        ),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(InlineKeyboardMarkup {
        inline_keyboard: vec![],
    })
    .await?;

    Ok(())
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
            send_cached_message(message, bot, download_data, need_delete_message).await
        }
        BotCache::NoCache => {
            send_with_download_from_channel(message, bot, download_data, need_delete_message).await
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
                    text: { format!("📥 {item}") },
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
    let allowed_langs = get_user_or_default_lang_codes(message.from().unwrap().id).await;

    let available_types = match command {
        DownloadArchiveCommand::Sequence { id } => {
            get_sequence_books_available_types(id, allowed_langs).await
        }
        DownloadArchiveCommand::Author { id } => {
            get_author_books_available_types(id, allowed_langs).await
        }
        DownloadArchiveCommand::Translator { id } => {
            get_translator_books_available_types(id, allowed_langs).await
        }
    };

    let available_types = match available_types {
        Ok(v) => v,
        Err(err) => return Err(err),
    };

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: available_types
            .iter()
            .filter(|file_type| !file_type.contains("zip"))
            .map(|file_type| {
                let callback_data: String = match command {
                    DownloadArchiveCommand::Sequence { id } => DownloadArchiveQueryData::Sequence {
                        id,
                        file_type: file_type.to_string(),
                    }
                    .to_string(),
                    DownloadArchiveCommand::Author { id } => DownloadArchiveQueryData::Author {
                        id,
                        file_type: file_type.to_string(),
                    }
                    .to_string(),
                    DownloadArchiveCommand::Translator { id } => {
                        DownloadArchiveQueryData::Translator {
                            id,
                            file_type: file_type.to_string(),
                        }
                        .to_string()
                    }
                };

                vec![InlineKeyboardButton {
                    text: file_type.to_string(),
                    kind: InlineKeyboardButtonKind::CallbackData(callback_data),
                }]
            })
            .collect(),
    };

    bot.send_message(message.chat.id, "Выбери формат:")
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

async fn send_archive_link(
    bot: CacheMe<Throttle<Bot>>,
    message: Message,
    task: Task,
) -> BotHandlerInternal {
    bot.edit_message_text(
        message.chat.id,
        message.id,
        format!(
            "Файл не может быть загружен в чат! \n \
                    Вы можете скачать его <a href=\"{}\">по ссылке</a> (работает 3 часа)",
            task.result_link.unwrap()
        ),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(InlineKeyboardMarkup {
        inline_keyboard: vec![],
    })
    .await?;

    Ok(())
}

async fn wait_archive(
    bot: CacheMe<Throttle<Bot>>,
    task_id: String,
    message: Message,
) -> BotHandlerInternal {
    let mut interval = time::interval(Duration::from_secs(15));

    let task = loop {
        interval.tick().await;

        let task = match get_task(task_id.clone()).await {
            Ok(v) => v,
            Err(err) => {
                send_error_message(bot, message.chat.id, message.id).await;
                log::error!("{:?}", err);
                return Err(err);
            }
        };

        if task.status != TaskStatus::InProgress {
            break task;
        }

        let now = Utc::now().format("%H:%M:%S UTC").to_string();

        bot.edit_message_text(
            message.chat.id,
            message.id,
            format!(
                "Статус: \n ⏳ {} \n\nОбновлено в {now}",
                task.status_description
            ),
        )
        .reply_markup(get_check_keyboard(task.id))
        .send()
        .await?;
    };

    if task.status != TaskStatus::Complete {
        send_error_message(bot, message.chat.id, message.id).await;
        return Ok(());
    }

    let content_size = task.content_size.unwrap();

    if content_size > 20 * 1024 * 1024 {
        send_archive_link(bot.clone(), message.clone(), task.clone()).await?;
        return Ok(());
    }

    let downloaded_data = match download_file_by_link(
        task.clone().result_filename.unwrap(),
        task.result_link.clone().unwrap(),
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            send_error_message(bot, message.chat.id, message.id).await;
            log::error!("{:?}", err);
            return Err(err);
        }
    };

    match _send_downloaded_file(&message, bot.clone(), downloaded_data).await {
        Ok(_) => (),
        Err(err) => {
            send_archive_link(bot.clone(), message.clone(), task).await?;
            log::error!("{:?}", err);
        }
    }

    bot.delete_message(message.chat.id, message.id).await?;

    Ok(())
}

async fn download_archive(
    cq: CallbackQuery,
    download_archive_query_data: DownloadArchiveQueryData,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(cq.from.id).await;

    let (id, file_type, task_type) = match download_archive_query_data {
        DownloadArchiveQueryData::Sequence { id, file_type } => {
            (id, file_type, TaskObjectType::Sequence)
        }
        DownloadArchiveQueryData::Author { id, file_type } => {
            (id, file_type, TaskObjectType::Author)
        }
        DownloadArchiveQueryData::Translator { id, file_type } => {
            (id, file_type, TaskObjectType::Translator)
        }
    };

    let message = cq.message.unwrap();

    let task = create_task(CreateTaskData {
        object_id: id,
        object_type: task_type,
        file_format: file_type,
        allowed_langs,
    })
    .await;

    let task = match task {
        Ok(v) => v,
        Err(err) => {
            send_error_message(bot, message.chat.id, message.id).await;
            log::error!("{:?}", err);
            return Err(err);
        }
    };

    bot.edit_message_text(message.chat.id, message.id, "⏳ Подготовка архива...")
        .reply_markup(get_check_keyboard(task.id.clone()))
        .send()
        .await?;

    let _ = wait_archive(bot, task.id, message).await;

    Ok(())
}

pub fn get_download_handler() -> crate::bots::BotHandler {
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
