pub mod callback_data;
pub mod commands;
pub mod errors;
pub mod formatter;
pub mod markup;
pub mod pages;

use book_bot_macros::log_handler;

use std::convert::TryInto;

use tracing::log;

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

const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_IMAGE_REDIRECTS: usize = 3;

fn is_forbidden_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.octets()[0] == 0
                // 100.64.0.0/10 (CGNAT)
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_forbidden_ip(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // fc00::/7 unique local, fe80::/10 link-local
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
        }
    }
}

/// Downloads an annotation image with SSRF protections: http/https only,
/// public IPs only (checked after DNS resolution and pinned for the
/// connection), bounded redirects, timeout and size cap.
async fn download_image(file: &str) -> anyhow::Result<bytes::Bytes> {
    use anyhow::{anyhow, bail};
    use futures::StreamExt;

    let mut url = reqwest::Url::parse(file)?;

    for _ in 0..=MAX_IMAGE_REDIRECTS {
        if !matches!(url.scheme(), "http" | "https") {
            bail!("unsupported image URL scheme");
        }
        let host = url
            .host_str()
            .ok_or_else(|| anyhow!("image URL has no host"))?
            .to_string();
        let port = url
            .port_or_known_default()
            .ok_or_else(|| anyhow!("image URL has no port"))?;

        let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
            .await?
            .collect();
        if addrs.is_empty() || addrs.iter().any(|a| is_forbidden_ip(a.ip())) {
            bail!("image host resolves to a forbidden address");
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .connect_timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .resolve_to_addrs(&host, &addrs)
            .build()?;

        let response = client.get(url.clone()).send().await?;

        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow!("redirect without Location"))?;
            url = url.join(location)?;
            continue;
        }

        let response = response.error_for_status()?;

        if response
            .content_length()
            .is_some_and(|len| len > MAX_IMAGE_BYTES as u64)
        {
            bail!("image too large");
        }

        let mut buf = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk?);
            if buf.len() > MAX_IMAGE_BYTES {
                bail!("image too large");
            }
        }
        return Ok(bytes::Bytes::from(buf));
    }

    bail!("too many redirects")
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

        match image_response {
            Ok(data) => {
                safe_send_photo(&bot, message.chat.id, InputFile::memory(data)).await?;
            }
            Err(err) => log::warn!("Annotation image download failed: {err}"),
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
            AnnotationCommand::Book { id } => AnnotationCallbackData::Book {
                id,
                page: 1,
                plain: false,
            },
            AnnotationCommand::Author { id } => AnnotationCallbackData::Author {
                id,
                page: 1,
                plain: false,
            },
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
        AnnotationCommand::Book { id } => AnnotationCallbackData::Book {
            id,
            page: 1,
            plain: true,
        },
        AnnotationCommand::Author { id } => AnnotationCallbackData::Author {
            id,
            page: 1,
            plain: true,
        },
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
    let (id, page, plain) = match callback_data {
        AnnotationCallbackData::Book { id, page, plain } => (id, page, plain),
        AnnotationCallbackData::Author { id, page, plain } => (id, page, plain),
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

    // The mode chosen when the message was first sent is persisted in the
    // callback data, so a rich-send fallback keeps paginating as plain text.
    let pages = if plain {
        AnnotationPages::Plain(build_pages(annotation_text, TELEGRAM_MESSAGE_MAX_LENGTH))
    } else {
        resolve_pages(annotation_text)
    };

    match pages {
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
