use futures::TryStreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncWriteExt;
use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InputFile, MaybeInaccessibleMessage, MessageId},
};
use tracing::log;

use crate::{
    bots::{
        approved_bot::{
            modules::utils::telegram_utils::{
                safe_copy_message, safe_delete_message, safe_send_document, safe_send_message,
            },
            services::{
                book_cache::{
                    download_file, get_cached_message,
                    types::{CachedMessage, DownloadFile},
                },
                donation_notifications::send_donation_notification,
            },
        },
        BotHandlerInternal,
    },
    bots_manager::BotCache,
};

use super::callback_data::DownloadQueryData;

async fn _send_cached(
    message: &MaybeInaccessibleMessage,
    bot: &CacheMe<Throttle<Bot>>,
    cached_message: CachedMessage,
) -> BotHandlerInternal {
    safe_copy_message(
        bot,
        ChatId(cached_message.chat_id),
        message.chat().id,
        MessageId(cached_message.message_id),
    )
    .await
}

pub async fn send_cached_message(
    message: MaybeInaccessibleMessage,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadQueryData,
    need_delete_message: bool,
    cache: BotCache,
    user_id: Option<u64>,
) -> BotHandlerInternal {
    'cached: {
        if let Ok(v) = get_cached_message(&download_data, cache, user_id).await {
            let cached = match v {
                Some(v) => v,
                None => break 'cached,
            };

            if _send_cached(&message, &bot, cached).await.is_ok() {
                if need_delete_message {
                    if let MaybeInaccessibleMessage::Regular(message) = &message {
                        let _ = safe_delete_message(&bot, message.chat.id, message.id).await;
                    }
                }

                match send_donation_notification(&bot, &message).await {
                    Ok(_) => (),
                    Err(err) => log::error!("{err:?}"),
                }

                return Ok(());
            }
        };
    }

    send_with_download_from_channel(message, bot, download_data, need_delete_message, user_id)
        .await?;

    Ok(())
}

pub async fn _send_downloaded_file(
    message: &MaybeInaccessibleMessage,
    bot: &CacheMe<Throttle<Bot>>,
    downloaded_data: DownloadFile,
) -> BotHandlerInternal {
    let DownloadFile {
        response,
        filename,
        caption,
    } = downloaded_data;

    // Stream into a temp file (bounded by an overall deadline) and send it
    // via `InputFile::file`, so Throttle retries re-open the file instead of
    // depending on a one-shot stream or buffering the whole body in memory.
    let path = std::env::temp_dir().join(format!(
        "book_bot_{}_{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let result = async {
        tokio::time::timeout(
            FILE_DOWNLOAD_DEADLINE,
            save_response_to_file(response, &path),
        )
        .await
        .map_err(|_| anyhow::anyhow!("file download deadline exceeded"))??;

        let document = InputFile::file(path.clone()).file_name(filename);
        safe_send_document(bot, message.chat().id, document, caption).await
    }
    .await;

    let _ = tokio::fs::remove_file(&path).await;

    if let Err(err) = result {
        notify_user(bot, message, BOOK_SEND_FAILED).await;
        return Err(err);
    }

    send_donation_notification(bot, message).await?;

    Ok(())
}

const BOOK_SEND_FAILED: &str =
    "Не удалось отправить файл, попробуй позже или выбери другой формат.";

const FILE_DOWNLOAD_DEADLINE: std::time::Duration = std::time::Duration::from_secs(600);

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

async fn save_response_to_file(
    response: reqwest::Response,
    path: &std::path::Path,
) -> anyhow::Result<()> {
    let mut file = tokio::fs::File::create(path).await?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.try_next().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

const BOOK_UNAVAILABLE_IN_FORMAT: &str =
    "Книга в этом формате сейчас недоступна. Попробуй другой формат.";
const BOOK_DOWNLOAD_FAILED: &str =
    "Не удалось скачать книгу, попробуй позже или выбери другой формат.";

/// Best-effort notice to the user; a failure here must not mask the original error.
async fn notify_user(bot: &CacheMe<Throttle<Bot>>, message: &MaybeInaccessibleMessage, text: &str) {
    if let Err(err) = safe_send_message(bot, message.chat().id, text, None).await {
        log::warn!("Failed to send download failure notice: {err:?}");
    }
}

pub async fn send_with_download_from_channel(
    message: MaybeInaccessibleMessage,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadQueryData,
    need_delete_message: bool,
    user_id: Option<u64>,
) -> BotHandlerInternal {
    let downloaded_file = match download_file(&download_data, user_id).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            notify_user(&bot, &message, BOOK_UNAVAILABLE_IN_FORMAT).await;
            return Ok(());
        }
        Err(err) => {
            notify_user(&bot, &message, BOOK_DOWNLOAD_FAILED).await;
            return Err(err);
        }
    };

    _send_downloaded_file(&message, &bot, downloaded_file).await?;

    if need_delete_message {
        if let MaybeInaccessibleMessage::Regular(message) = message {
            let _ = safe_delete_message(&bot, message.chat.id, message.id).await;
        };
    }

    Ok(())
}

pub async fn download_handler(
    message: MaybeInaccessibleMessage,
    bot: CacheMe<Throttle<Bot>>,
    cache: BotCache,
    download_data: DownloadQueryData,
    need_delete_message: bool,
    user_id: Option<u64>,
) -> BotHandlerInternal {
    match cache {
        BotCache::Original | BotCache::Cache => {
            send_cached_message(
                message,
                bot,
                download_data,
                need_delete_message,
                cache,
                user_id,
            )
            .await
        }
        BotCache::NoCache => {
            send_with_download_from_channel(
                message,
                bot,
                download_data,
                need_delete_message,
                user_id,
            )
            .await
        }
    }
}
