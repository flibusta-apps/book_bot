pub mod archive;
pub mod callback_data;
pub mod commands;
pub mod file_send;
pub mod keyboards;

use super::utils::constants::*;
use super::utils::telegram_utils::safe_send_message_with_reply;

use book_bot_macros::log_handler;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::UpdateFilterExt,
    dptree,
    prelude::*,
    types::*,
};

use crate::bots::{
    approved_bot::{
        services::{
            book_library::{
                get_author_books_available_types, get_book, get_sequence_books_available_types,
                get_translator_books_available_types,
            },
            user_settings::get_user_or_default_lang_codes,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};
use crate::bots_manager::BotCache;

use self::{
    archive::download_archive,
    callback_data::{CheckArchiveStatus, DownloadArchiveQueryData, DownloadQueryData},
    commands::{DownloadArchiveCommand, StartDownloadCommand},
    file_send::download_handler,
    keyboards::{get_download_archive_format_keyboard, get_download_format_keyboard},
};

use super::utils::filter_command::filter_command;

use archive::wait_archive;

#[log_handler("download")]
async fn get_download_keyboard_handler(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    download_data: StartDownloadCommand,
) -> BotHandlerInternal {
    let book = match get_book(download_data.id).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message_with_reply(
                &bot,
                message.chat.id,
                NOT_FOUND,
                ReplyParameters::new(message.id),
                None,
            )
            .await?;
            return Ok(());
        }
        Err(err) => {
            safe_send_message_with_reply(
                &bot,
                message.chat.id,
                ERROR_TRY_LATER,
                ReplyParameters::new(message.id),
                None,
            )
            .await?;

            return Err(err);
        }
    };

    let keyboard = get_download_format_keyboard(&book);

    safe_send_message_with_reply(
        &bot,
        message.chat.id,
        "Выбери формат:",
        ReplyParameters::new(message.id),
        Some(keyboard),
    )
    .await?;

    Ok(())
}

#[log_handler("download")]
async fn get_download_archive_keyboard_handler(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    command: DownloadArchiveCommand,
) -> BotHandlerInternal {
    let Some(from) = message.from.as_ref() else {
        return Ok(());
    };
    let allowed_langs = get_user_or_default_lang_codes(from.id).await;

    let available_types = match command {
        DownloadArchiveCommand::Sequence { id } => {
            get_sequence_books_available_types(id, &allowed_langs).await
        }
        DownloadArchiveCommand::Author { id } => {
            get_author_books_available_types(id, &allowed_langs).await
        }
        DownloadArchiveCommand::Translator { id } => {
            get_translator_books_available_types(id, &allowed_langs).await
        }
    };

    let available_types = match available_types {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message_with_reply(
                &bot,
                message.chat.id,
                NOT_FOUND,
                ReplyParameters::new(message.id),
                None,
            )
            .await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let keyboard = get_download_archive_format_keyboard(command, &available_types);

    safe_send_message_with_reply(
        &bot,
        message.chat.id,
        "Выбери формат:",
        ReplyParameters::new(message.id),
        Some(keyboard),
    )
    .await?;

    Ok(())
}

#[log_handler("download")]
async fn download_query_handler(
    cq: CallbackQuery,
    download_query_data: DownloadQueryData,
    bot: CacheMe<Throttle<Bot>>,
    cache: BotCache,
) -> BotHandlerInternal {
    let Some(message) = cq.message else {
        return Ok(());
    };
    let user_id = Some(cq.from.id.0);
    download_handler(message, bot, cache, download_query_data, true, user_id).await
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
                .endpoint(download_query_handler),
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
                let Some(message) = cq.message else {
                    return Ok(());
                };
                wait_archive(bot, status.task_id, message).await
            })
        )
}
