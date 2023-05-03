use futures::TryStreamExt;
use regex::Regex;
use teloxide::{dispatching::UpdateFilterExt, dptree, prelude::*, types::*};
use tokio_util::compat::FuturesAsyncReadCompatExt;

use crate::{
    bots::{
        approved_bot::services::book_cache::{
            download_file, get_cached_message,
            types::{CachedMessage, DownloadFile},
        },
        BotHandlerInternal,
    },
    bots_manager::BotCache,
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
    bot: &Bot,
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
    bot: Bot,
    download_data: DownloadData,
) -> BotHandlerInternal {
    if let Ok(v) = get_cached_message(&download_data).await {
        if let Ok(_) = _send_cached(&message, &bot, v).await {
            return Ok(());
        }
    };

    match send_with_download_from_channel(message, bot, download_data).await {
        Ok(_) => Ok(()),
        Err(err) => Err(err),
    }
}

async fn _send_downloaded_file(
    message: &Message,
    bot: Bot,
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

    match bot
        .send_document(message.chat.id, document)
        .caption(caption)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(Box::new(err)),
    }
}

async fn send_with_download_from_channel(
    message: Message,
    bot: Bot,
    download_data: DownloadData,
) -> BotHandlerInternal {
    match download_file(&download_data).await {
        Ok(v) => match _send_downloaded_file(&message, bot, v).await {
            Ok(v_2) => Ok(v_2),
            Err(err) => Err(err),
        },
        Err(err) => Err(err),
    }
}

async fn download_handler(
    message: Message,
    bot: Bot,
    cache: BotCache,
    download_data: DownloadData,
) -> BotHandlerInternal {
    match cache {
        BotCache::Original => send_cached_message(message, bot, download_data).await,
        BotCache::NoCache => send_with_download_from_channel(message, bot, download_data).await,
    }
}

pub fn get_download_hander() -> crate::bots::BotHandler {
    dptree::entry().branch(
        Update::filter_message()
            .chain(filter_command::<DownloadData>())
            .endpoint(
                |message: Message,
                 bot: Bot,
                 cache: BotCache,
                 download_data: DownloadData| async move {
                    download_handler(message, bot, cache, download_data).await
                },
            ),
    )
}
