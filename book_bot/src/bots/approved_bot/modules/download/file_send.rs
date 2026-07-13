use futures::TryStreamExt;
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
                safe_copy_message, safe_delete_message, safe_send_document,
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

    let stream = response.bytes_stream().map_err(std::io::Error::other);
    let data = tokio_util::io::StreamReader::new(stream);

    let document = InputFile::read(data).file_name(filename);

    safe_send_document(bot, message.chat().id, document, caption).await?;

    send_donation_notification(bot, message).await?;

    Ok(())
}

pub async fn send_with_download_from_channel(
    message: MaybeInaccessibleMessage,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadQueryData,
    need_delete_message: bool,
    user_id: Option<u64>,
) -> BotHandlerInternal {
    let downloaded_file = match download_file(&download_data, user_id).await? {
        Some(v) => v,
        None => {
            return Ok(());
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
