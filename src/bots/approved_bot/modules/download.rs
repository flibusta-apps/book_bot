use futures::TryStreamExt;
use moka::future::Cache;
use regex::Regex;
use teloxide::{dispatching::UpdateFilterExt, dptree, prelude::*, types::*, adaptors::{Throttle, CacheMe}};
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::{
    bots::{
        approved_bot::services::{book_cache::{
            download_file, get_cached_message,
            types::{CachedMessage, DownloadFile},
        }, donation_notificatioins::send_donation_notification},
        BotHandlerInternal,
    },
    bots_manager::{BotCache, AppState},
};

use super::utils::{filter_command, CommandParse};

#[derive(Clone)]
pub struct DownloadData {
    pub format: String,
    pub id: u32,
}

impl CommandParse<Self> for DownloadData {
    fn parse(s: &str, bot_name: &str) -> Result<Self, strum::ParseError> {
        let re = Regex::new(r"^/d_(?P<file_format>[a-zA-Z0-9]+)_(?P<book_id>\d+)$").unwrap();

        let full_bot_name = format!("@{bot_name}");
        let after_replace = s.replace(&full_bot_name, "");

        let caps = re.captures(&after_replace);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let file_format = &caps["file_format"];
        let book_id: u32 = caps["book_id"].parse().unwrap();

        Ok(DownloadData {
            format: file_format.to_string(),
            id: book_id,
        })
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
    download_data: DownloadData,
    donation_notification_cache: Cache<ChatId, bool>,
) -> BotHandlerInternal {
    if let Ok(v) = get_cached_message(&download_data).await {
        if _send_cached(&message, &bot, v).await.is_ok() {
            send_donation_notification(bot.clone(), message, donation_notification_cache).await?;

            return Ok(());
        }
    };

    send_with_download_from_channel(message, bot, download_data, donation_notification_cache).await?;

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

    bot
        .send_document(message.chat.id, document)
        .caption(caption)
        .send()
        .await?;

    send_donation_notification(bot, message.clone(), donation_notification_cache).await?;

    Ok(())
}

async fn send_with_download_from_channel(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadData,
    donation_notification_cache: Cache<ChatId, bool>,
) -> BotHandlerInternal {
    match download_file(&download_data).await {
        Ok(v) => Ok(_send_downloaded_file(&message, bot, v, donation_notification_cache).await?),
        Err(err) => Err(err),
    }
}

async fn download_handler(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    cache: BotCache,
    download_data: DownloadData,
    donation_notification_cache: Cache<ChatId, bool>,
) -> BotHandlerInternal {
    match cache {
        BotCache::Original => send_cached_message(message, bot, download_data, donation_notification_cache).await,
        BotCache::NoCache => send_with_download_from_channel(message, bot, download_data, donation_notification_cache).await,
    }
}

pub fn get_download_hander() -> crate::bots::BotHandler {
    dptree::entry().branch(
        Update::filter_message()
            .chain(filter_command::<DownloadData>())
            .endpoint(
                |message: Message,
                 bot: CacheMe<Throttle<Bot>>,
                 cache: BotCache,
                 download_data: DownloadData,
                 app_state: AppState| async move {
                    download_handler(
                        message,
                        bot,
                        cache,
                        download_data,
                        app_state.chat_donation_notifications_cache
                    ).await
                },
            ),
    )
}
