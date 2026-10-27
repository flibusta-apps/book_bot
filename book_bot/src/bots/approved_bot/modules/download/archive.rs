use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use book_bot_macros::log_handler;
use chrono::Utc;
use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage, MessageId},
};
use tokio::time;
use tracing::log;

use crate::{
    bots::{
        approved_bot::{
            modules::utils::{
                constants::*,
                telegram_utils::{
                    safe_delete_message, safe_edit_message_text, safe_edit_message_text_html,
                },
            },
            services::{
                batch_downloader::{
                    create_task, get_task, CreateTaskData, QueueFull, Task, TaskObjectType,
                    TaskStatus,
                },
                book_cache::download_file_by_link,
                build_url,
                user_settings::{
                    get_user_file_name_lang_for, get_user_or_default_lang_codes, FileNameLang,
                },
            },
        },
        BotHandlerInternal,
    },
    config,
};

use super::{
    callback_data::DownloadArchiveQueryData, file_send::_send_downloaded_file,
    keyboards::get_check_keyboard,
};

async fn send_error_message(bot: &CacheMe<Throttle<Bot>>, chat_id: ChatId, message_id: MessageId) {
    let _ = safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        ERROR_TRY_LATER,
        Some(InlineKeyboardMarkup::new(
            Vec::<Vec<InlineKeyboardButton>>::new(),
        )),
    )
    .await;
}

async fn send_archive_link(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    task: &Task,
) -> BotHandlerInternal {
    let link = build_url(
        &config::CONFIG.public_batch_downloader_url,
        ["api", "download", &task.id],
    )?
    .to_string();

    safe_edit_message_text_html(
        bot,
        chat_id,
        message_id,
        format!(
            "Файл не может быть загружен в чат! \n \
                    Вы можете скачать его <a href=\"{link}\">по ссылке</a> (работает 3 часа)"
        ),
        Some(InlineKeyboardMarkup::new(
            Vec::<Vec<InlineKeyboardButton>>::new(),
        )),
    )
    .await?;

    Ok(())
}

/// Task ids that currently have a background waiter / finisher running.
static ACTIVE_TASKS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

struct ActiveTaskGuard(String);

impl ActiveTaskGuard {
    /// Returns `None` if the task is already being handled.
    fn acquire(task_id: &str) -> Option<Self> {
        let mut set = ACTIVE_TASKS.lock().unwrap_or_else(|e| e.into_inner());
        if set.insert(task_id.to_string()) {
            Some(Self(task_id.to_string()))
        } else {
            None
        }
    }
}

impl Drop for ActiveTaskGuard {
    fn drop(&mut self) {
        ACTIVE_TASKS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

fn archive_wait_max() -> Duration {
    static MAX: LazyLock<Duration> = LazyLock::new(|| {
        let secs = std::env::var("ARCHIVE_WAIT_MAX_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1800);
        Duration::from_secs(secs)
    });
    *MAX
}

fn is_in_progress(task: &Task) -> bool {
    matches!(task.status, TaskStatus::InProgress | TaskStatus::Archiving)
}

async fn show_progress(
    bot: &CacheMe<Throttle<Bot>>,
    message: &Message,
    task: &Task,
) -> BotHandlerInternal {
    let now = Utc::now().format("%H:%M:%S UTC").to_string();

    safe_edit_message_text(
        bot,
        message.chat.id,
        message.id,
        format!(
            "Статус: \n ⏳ {} \n\nОбновлено в {now}",
            task.status_description
        ),
        Some(get_check_keyboard(task.id.clone())),
    )
    .await?;

    Ok(())
}

/// Polls the task until it leaves the in-progress states or the total
/// deadline (`ARCHIVE_WAIT_MAX_SECS`, default 1800) expires, then delivers
/// the result. Runs in a background task, one per task id.
pub async fn wait_archive(
    bot: CacheMe<Throttle<Bot>>,
    task_id: String,
    input_message: MaybeInaccessibleMessage,
) -> BotHandlerInternal {
    let message = match input_message {
        MaybeInaccessibleMessage::Regular(message) => message,
        _ => {
            send_error_message(&bot, input_message.chat().id, input_message.id()).await;
            return Ok(());
        }
    };

    let Some(_guard) = ActiveTaskGuard::acquire(&task_id) else {
        return Ok(());
    };

    let deadline = time::Instant::now() + archive_wait_max();
    let mut interval = time::interval(Duration::from_secs(15));

    let task = loop {
        interval.tick().await;

        let task = match get_task(&task_id).await {
            Ok(v) => v,
            Err(err) => {
                send_error_message(&bot, message.chat.id, message.id).await;
                log::error!("{err:?}");
                return Err(err);
            }
        };

        if !is_in_progress(&task) {
            break task;
        }

        if time::Instant::now() >= deadline {
            log::warn!("Archive task {task_id} wait deadline exceeded");
            send_error_message(&bot, message.chat.id, message.id).await;
            return Ok(());
        }

        show_progress(&bot, &message, &task).await?;
    };

    finish_task(&bot, &message, task).await
}

/// Handles the "check status" button: a single `get_task`, no new loop.
pub async fn check_archive_status(
    bot: CacheMe<Throttle<Bot>>,
    task_id: String,
    input_message: MaybeInaccessibleMessage,
) -> BotHandlerInternal {
    let message = match input_message {
        MaybeInaccessibleMessage::Regular(message) => message,
        _ => {
            send_error_message(&bot, input_message.chat().id, input_message.id()).await;
            return Ok(());
        }
    };

    let task = match get_task(&task_id).await {
        Ok(v) => v,
        Err(err) => {
            send_error_message(&bot, message.chat.id, message.id).await;
            log::error!("{err:?}");
            return Err(err);
        }
    };

    if is_in_progress(&task) {
        return show_progress(&bot, &message, &task).await;
    }

    // Finished. If a background waiter is active it will deliver the result.
    let Some(_guard) = ActiveTaskGuard::acquire(&task_id) else {
        return Ok(());
    };

    finish_task(&bot, &message, task).await
}

async fn finish_task(
    bot: &CacheMe<Throttle<Bot>>,
    message: &Message,
    task: Task,
) -> BotHandlerInternal {
    let bot = bot.clone();
    if task.status == TaskStatus::Failed {
        let is_rate_limit = task
            .error_message
            .as_deref()
            .map(|msg| msg.to_lowercase().contains("rate limit"))
            .unwrap_or(false);

        if is_rate_limit {
            log::warn!(
                "Rate limit hit for user {} on task {}",
                message.chat.id,
                task.id
            );
            let _ = safe_edit_message_text(
                &bot,
                message.chat.id,
                message.id,
                RATE_LIMIT_ERROR,
                Some(InlineKeyboardMarkup::new(
                    Vec::<Vec<InlineKeyboardButton>>::new(),
                )),
            )
            .await;
        } else {
            log::error!("Task {} failed: {:?}", task.id, task.error_message);
            send_error_message(&bot, message.chat.id, message.id).await;
        }
        return Ok(());
    }

    if task.status != TaskStatus::Complete {
        send_error_message(&bot, message.chat.id, message.id).await;
        return Ok(());
    }

    let Some(content_size) = task.content_size else {
        send_archive_link(&bot, message.chat.id, message.id, &task).await?;
        return Ok(());
    };

    if content_size > 1024 * 1024 * 1024 {
        send_archive_link(&bot, message.chat.id, message.id, &task).await?;
        return Ok(());
    }

    let link = build_url(
        &config::CONFIG.batch_downloader_url,
        ["api", "download", &task.id],
    )?
    .to_string();

    let downloaded_data = match download_file_by_link(
        task.result_filename.as_deref().unwrap_or_default(),
        link,
    )
    .await
    {
        Ok(v) => match v {
            Some(v) => v,
            None => {
                send_error_message(&bot, message.chat.id, message.id).await;
                return Ok(());
            }
        },
        Err(err) => {
            send_error_message(&bot, message.chat.id, message.id).await;
            log::warn!("{err:?}");
            return Err(err);
        }
    };

    match _send_downloaded_file(
        &MaybeInaccessibleMessage::Regular(Box::new(message.clone())),
        &bot,
        downloaded_data,
    )
    .await
    {
        Ok(_) => (),
        Err(err) => {
            log::warn!("{err:?}");
            send_archive_link(&bot, message.chat.id, message.id, &task).await?;
            return Ok(());
        }
    }

    let _ = safe_delete_message(&bot, message.chat.id, message.id).await;

    Ok(())
}

#[log_handler("download")]
pub async fn download_archive(
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

    let Some(message) = cq.message else {
        return Ok(());
    };

    let user_id = cq.from.id.0;

    // `normalized` mirrors the cache server's `?normalized=` parameter.
    // Default for the server is `true` (transliterated names); we send
    // `false` only when the user opted into original Cyrillic names.
    let normalized = !matches!(
        get_user_file_name_lang_for(Some(user_id)).await,
        FileNameLang::Original
    );

    let task = create_task(
        CreateTaskData {
            object_id: id,
            object_type: task_type,
            file_format: file_type,
            allowed_langs,
            normalized,
        },
        Some(user_id),
    )
    .await;

    let task = match task {
        Ok(v) => v,
        Err(err) => {
            if let Some(full) = err.downcast_ref::<QueueFull>() {
                log::warn!("{err}");
                let _ = safe_edit_message_text(
                    &bot,
                    message.chat().id,
                    message.id(),
                    format!(
                        "Сервис архивов перегружен, попробуйте через {} сек",
                        full.retry_after_secs
                    ),
                    Some(InlineKeyboardMarkup::new(
                        Vec::<Vec<InlineKeyboardButton>>::new(),
                    )),
                )
                .await;
                return Ok(());
            }
            send_error_message(&bot, message.chat().id, message.id()).await;
            log::error!("{err:?}");
            return Err(err);
        }
    };

    safe_edit_message_text(
        &bot,
        message.chat().id,
        message.id(),
        "⏳ Подготовка архива...",
        Some(get_check_keyboard(task.id.clone())),
    )
    .await?;

    let task_id = task.id;
    tokio::spawn(async move {
        if let Err(err) = wait_archive(bot, task_id, message).await {
            log::error!("{err:?}");
        }
    });

    Ok(())
}
