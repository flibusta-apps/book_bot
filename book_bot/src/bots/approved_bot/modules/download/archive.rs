use std::time::Duration;

use book_bot_macros::log_handler;
use chrono::Utc;
use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InlineKeyboardMarkup, MaybeInaccessibleMessage, MessageId},
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
                    create_task, get_task, CreateTaskData, Task, TaskObjectType, TaskStatus,
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
        Some(InlineKeyboardMarkup {
            inline_keyboard: vec![],
        }),
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
        Some(InlineKeyboardMarkup {
            inline_keyboard: vec![],
        }),
    )
    .await?;

    Ok(())
}

pub async fn wait_archive(
    bot: CacheMe<Throttle<Bot>>,
    task_id: String,
    input_message: MaybeInaccessibleMessage,
) -> BotHandlerInternal {
    let mut interval = time::interval(Duration::from_secs(15));

    let message = match input_message {
        MaybeInaccessibleMessage::Regular(message) => message,
        _ => {
            send_error_message(&bot, input_message.chat().id, input_message.id()).await;
            return Ok(());
        }
    };

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

        if !matches!(task.status, TaskStatus::InProgress | TaskStatus::Archiving) {
            break task;
        }

        let now = Utc::now().format("%H:%M:%S UTC").to_string();

        safe_edit_message_text(
            &bot,
            message.chat.id,
            message.id,
            format!(
                "Статус: \n ⏳ {} \n\nОбновлено в {now}",
                task.status_description
            ),
            Some(get_check_keyboard(task.id)),
        )
        .await?;
    };

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
                Some(InlineKeyboardMarkup {
                    inline_keyboard: vec![],
                }),
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
        &MaybeInaccessibleMessage::Regular(message.clone()),
        &bot,
        downloaded_data,
    )
    .await
    {
        Ok(_) => (),
        Err(err) => {
            send_archive_link(&bot, message.chat.id, message.id, &task).await?;
            log::warn!("{err:?}");
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

    if let Err(err) = wait_archive(bot, task.id, message).await {
        log::error!("{err:?}");
    }

    Ok(())
}
