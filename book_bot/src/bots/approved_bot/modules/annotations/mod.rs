pub mod callback_data;
pub mod commands;
pub mod errors;
pub mod formatter;
pub mod markup;
pub mod pages;

use book_bot_macros::log_handler;

use std::convert::TryInto;

use futures::TryStreamExt;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    dispatching::UpdateFilterExt,
    dptree,
    prelude::*,
    types::*,
};

use crate::bots::{
    approved_bot::{
        modules::utils::{
            message_text::is_message_text_equals,
            pagination::generic_get_pagination_keyboard,
            telegram_utils::{
                safe_edit_message_text_html_with_fallback, safe_edit_rich_message_text,
                safe_send_message_with_reply, safe_send_message_with_reply_html, safe_send_photo,
                safe_send_rich_message_with_reply,
            },
        },
        services::book_library::{get_author_annotation, get_book_annotation},
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use self::{
    callback_data::AnnotationCallbackData,
    commands::AnnotationCommand,
    errors::AnnotationFormatError,
    formatter::AnnotationFormat,
    pages::{build_pages, build_rich_pages, plain_text_len},
};

/// Empirically-verified-safe cumulative plain-text budget (UTF-16 units)
/// across all blocks in a single `InputRichMessage`. Live-tested against
/// this bot's actual Bot API server: 30,000 chars succeeded (verified
/// byte-perfect via `forwardMessage`), 33,000 failed with
/// `RICH_MESSAGE_TEXT_TOO_LONG`. 25,000 leaves a comfortable safety margin
/// below that measured boundary -- do not raise this without re-verifying
/// against a live Bot API server.
const RICH_MESSAGE_TOTAL_BUDGET: usize = 25_000;

use super::utils::{constants::TELEGRAM_MESSAGE_MAX_LENGTH, filter_command::filter_command};

async fn download_image(
    file: &String,
) -> Result<reqwest::Response, Box<dyn std::error::Error + Send + Sync>> {
    Ok(reqwest::get(file).await?.error_for_status()?)
}

/// Which page representation a given annotation's text was (or would be)
/// paginated as. `send_annotation_handler` and `annotation_pagination_handler`
/// must always agree on this for the same annotation text, since
/// `build_rich_pages` and `build_pages` produce a *different* number of
/// pages for the same input (rich pages are much bigger), so a `page`
/// index from a rich-paginated message is meaningless if resolved against
/// `build_pages` and vice versa. Both handlers call [`resolve_pages`] with
/// the identical decision criteria so they always agree.
enum AnnotationPages {
    Rich(Vec<pages::RichPage>),
    Plain(Vec<pages::Page>),
}

/// Decide which pagination representation an annotation's text uses, and
/// build it. Prefers the rich-blocks representation whenever the text is
/// too long for a single normal message *and* `build_rich_pages` actually
/// produces at least one page for it; otherwise (short text, or the rare
/// case where `build_rich_pages` yields nothing) falls back to the plain
/// `build_pages` representation.
fn resolve_pages(annotation_text: &str) -> AnnotationPages {
    if plain_text_len(annotation_text) > TELEGRAM_MESSAGE_MAX_LENGTH {
        let rich_pages = build_rich_pages(
            annotation_text,
            TELEGRAM_MESSAGE_MAX_LENGTH,
            RICH_MESSAGE_TOTAL_BUDGET,
        );
        if !rich_pages.is_empty() {
            return AnnotationPages::Rich(rich_pages);
        }
    }

    AnnotationPages::Plain(build_pages(annotation_text, TELEGRAM_MESSAGE_MAX_LENGTH))
}

#[log_handler("annotations")]
pub async fn send_annotation_handler<T, Fut>(
    message: Message,
    bot: CacheMe<Throttle<Bot>>,
    command: AnnotationCommand,
    annotation_getter: fn(id: u32) -> Fut,
) -> BotHandlerInternal
where
    T: AnnotationFormat,
    Fut: std::future::Future<Output = anyhow::Result<Option<T>>>,
{
    let id = match command {
        AnnotationCommand::Book { id } => id,
        AnnotationCommand::Author { id } => id,
    };

    let annotation = match annotation_getter(id).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            return safe_send_message_with_reply(
                &bot,
                message.chat.id,
                "Аннотация недоступна :(",
                ReplyParameters::new(message.id),
                None,
            )
            .await;
        }
        Err(err) => return Err(err),
    };

    if annotation.get_file().is_none() && !annotation.is_normal_text() {
        return safe_send_message_with_reply(
            &bot,
            message.chat.id,
            "Аннотация недоступна :(",
            ReplyParameters::new(message.id),
            None,
        )
        .await;
    };

    if let Some(file) = annotation.get_file() {
        let image_response = download_image(file).await;

        if let Ok(v) = image_response {
            let stream = v.bytes_stream().map_err(std::io::Error::other);
            let data = tokio_util::io::StreamReader::new(stream);

            safe_send_photo(&bot, message.chat.id, InputFile::read(data)).await?;
        }
    };

    if !annotation.is_normal_text() {
        return Err(AnnotationFormatError {
            command,
            text: annotation.get_text().to_string(),
        }
        .into());
    }

    let annotation_text = annotation.get_text();

    if let AnnotationPages::Rich(rich_pages) = resolve_pages(annotation_text) {
        // `resolve_pages` only returns `Rich` when `rich_pages` is
        // non-empty, so `first()` is guaranteed to succeed here.
        let first_rich_page = rich_pages
            .first()
            .expect("resolve_pages guarantees non-empty");

        let callback_data = match command {
            AnnotationCommand::Book { id } => AnnotationCallbackData::Book { id, page: 1 },
            AnnotationCommand::Author { id } => AnnotationCallbackData::Author { id, page: 1 },
        };
        let keyboard =
            generic_get_pagination_keyboard(1, rich_pages.len().try_into()?, callback_data, false);

        match safe_send_rich_message_with_reply(
            &bot,
            message.chat.id,
            first_rich_page.blocks.clone(),
            ReplyParameters::new(message.id),
            Some(keyboard),
        )
        .await
        {
            Ok(true) | Ok(false) => return Ok(()),
            Err(_) => {
                // Fall back to the plain-text paginated approach below.
            }
        }
    }

    let pages = build_pages(annotation_text, TELEGRAM_MESSAGE_MAX_LENGTH);
    let current_page = match pages.first() {
        Some(p) => p,
        None => {
            return safe_send_message_with_reply(
                &bot,
                message.chat.id,
                "Аннотация недоступна :(",
                ReplyParameters::new(message.id),
                None,
            )
            .await;
        }
    };

    let callback_data = match command {
        AnnotationCommand::Book { id } => AnnotationCallbackData::Book { id, page: 1 },
        AnnotationCommand::Author { id } => AnnotationCallbackData::Author { id, page: 1 },
    };
    let keyboard =
        generic_get_pagination_keyboard(1, pages.len().try_into()?, callback_data, false);

    safe_send_message_with_reply_html(
        &bot,
        message.chat.id,
        current_page.html.clone(),
        current_page.plain.clone(),
        ReplyParameters::new(message.id),
        Some(keyboard),
    )
    .await?;

    Ok(())
}

#[log_handler("annotations")]
pub async fn annotation_pagination_handler<T, Fut>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    callback_data: AnnotationCallbackData,
    annotation_getter: fn(id: u32) -> Fut,
) -> BotHandlerInternal
where
    T: AnnotationFormat,
    Fut: std::future::Future<Output = anyhow::Result<Option<T>>>,
{
    let (id, page) = match callback_data {
        AnnotationCallbackData::Book { id, page } => (id, page),
        AnnotationCallbackData::Author { id, page } => (id, page),
    };

    let annotation = match annotation_getter(id).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let message = match cq.message {
        Some(v) => v,
        None => return Ok(()),
    };

    let request_page: usize = page.try_into().unwrap_or(1);

    let annotation_text = annotation.get_text();

    match resolve_pages(annotation_text) {
        AnnotationPages::Rich(rich_pages) => {
            let page_index = if request_page <= rich_pages.len() {
                request_page
            } else {
                rich_pages.len()
            };

            let new_page = match rich_pages.get(page_index.saturating_sub(1)) {
                Some(p) => p,
                None => return Ok(()),
            };

            let keyboard = generic_get_pagination_keyboard(
                page,
                rich_pages.len().try_into()?,
                callback_data,
                false,
            );

            // `Message::text()` is entity-stripped plain text; it's not
            // clear from the teloxide type definitions alone whether
            // Telegram also populates an equivalent flattened plain-text
            // field for Rich Messages (vs. the structured
            // `Message::rich_message`). Rather than guess at that
            // behaviour, skip the no-op-edit optimization here and always
            // issue the edit -- the only cost is a redundant `editMessageText`
            // call when a user re-clicks the currently displayed page.
            safe_edit_rich_message_text(
                &bot,
                message.chat().id,
                message.id(),
                new_page.blocks.clone(),
                Some(keyboard),
            )
            .await
            .map(|_| ())
        }
        AnnotationPages::Plain(pages) => {
            let page_index = if request_page <= pages.len() {
                request_page
            } else {
                pages.len()
            };

            let new_page = match pages.get(page_index.saturating_sub(1)) {
                Some(p) => p,
                None => return Ok(()),
            };

            let keyboard = generic_get_pagination_keyboard(
                page,
                pages.len().try_into()?,
                callback_data,
                false,
            );

            if is_message_text_equals(Some(message.clone()), &new_page.plain) {
                return Ok(());
            }

            safe_edit_message_text_html_with_fallback(
                &bot,
                message.chat().id,
                message.id(),
                new_page.html.clone(),
                new_page.plain.clone(),
                Some(keyboard),
            )
            .await
        }
    }
}

pub fn get_annotations_handler() -> crate::bots::BotHandler {
    dptree::entry()
        .branch(
            Update::filter_message()
                .chain(filter_command::<AnnotationCommand>())
                .endpoint(
                    |message: Message, bot: CacheMe<Throttle<Bot>>, command: AnnotationCommand| async move {
                        match command {
                            AnnotationCommand::Book { .. } => {
                                send_annotation_handler(message, bot, command, get_book_annotation)
                                    .await
                            }
                            AnnotationCommand::Author { .. } => {
                                send_annotation_handler(
                                    message,
                                    bot,
                                    command,
                                    get_author_annotation,
                                )
                                .await
                            }
                        }
                    },
                ),
        )
        .branch(
            Update::filter_callback_query()
                .chain(filter_callback_query::<AnnotationCallbackData>())
                .endpoint(
                    |cq: CallbackQuery,
                     bot: CacheMe<Throttle<Bot>>,
                     callback_data: AnnotationCallbackData| async move {
                        match callback_data {
                            AnnotationCallbackData::Book { .. } => {
                                annotation_pagination_handler(
                                    cq,
                                    bot,
                                    callback_data,
                                    get_book_annotation,
                                )
                                .await
                            }
                            AnnotationCallbackData::Author { .. } => {
                                annotation_pagination_handler(
                                    cq,
                                    bot,
                                    callback_data,
                                    get_author_annotation,
                                )
                                .await
                            }
                        }
                    },
                ),
        )
}
