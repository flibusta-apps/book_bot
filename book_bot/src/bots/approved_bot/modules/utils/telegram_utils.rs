use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{
        CallbackQueryId, InlineKeyboardMarkup, InputFile, InputRichBlock, InputRichMessage,
        MessageId, ParseMode, ReplyParameters,
    },
    ApiError, RequestError,
};

use tracing::log;

use crate::bots::BotHandlerInternal;

/// Safely edit message text, handling common Telegram API errors.
///
/// - `MessageNotModified` → Ok(()) (content unchanged, nothing to do)
/// - `MessageToEditNotFound` / `MessageIdInvalid` → send new message as fallback
/// - `NotEnoughRights*` / `MessageTextIsEmpty` → Ok(()) (can't act, suppress)
/// - Other errors → Err
pub async fn safe_edit_message_text(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    text: impl Into<String>,
    keyboard: Option<InlineKeyboardMarkup>,
) -> BotHandlerInternal {
    let text = text.into();
    let mut request = bot
        .edit_message_text(chat_id, message_id)
        .text(text.clone());

    if let Some(ref keyboard) = keyboard {
        request = request.reply_markup(keyboard.clone());
    }

    match request.send().await {
        Ok(_) => Ok(()),
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::MessageNotModified => Ok(()),
            ApiError::MessageToEditNotFound | ApiError::MessageIdInvalid => {
                // Original message was deleted, send as new message
                let mut send_request = bot.send_message(chat_id, &text);
                if let Some(keyboard) = keyboard {
                    send_request = send_request.reply_markup(keyboard);
                }
                match send_request.send().await {
                    Ok(_) => Ok(()),
                    Err(RequestError::Api(
                        ApiError::NotEnoughRightsToPostMessages
                        | ApiError::NotEnoughRightsToRestrict
                        | ApiError::NotEnoughRightsToChangeChatPermissions
                        | ApiError::NotEnoughRightsToManagePins
                        | ApiError::NotEnoughRightsToPinMessage
                        | ApiError::MessageTextIsEmpty,
                    )) => Ok(()),
                    Err(e) => Err(e.into()),
                }
            }
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage
            | ApiError::MessageTextIsEmpty => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely edit message reply markup, handling common Telegram API errors.
///
/// - `MessageNotModified` → Ok(()) (markup unchanged, nothing to do)
/// - `MessageToEditNotFound` / `MessageIdInvalid` → Ok(()) (message deleted, keyboard irrelevant)
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - Other errors → Err
pub async fn safe_edit_message_reply_markup(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    keyboard: InlineKeyboardMarkup,
) -> BotHandlerInternal {
    match bot
        .edit_message_reply_markup(chat_id, message_id)
        .reply_markup(keyboard)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::MessageNotModified
            | ApiError::MessageToEditNotFound
            | ApiError::MessageIdInvalid
            | ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely send a message, handling common Telegram API errors.
///
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - `MessageTextIsEmpty` → Ok(()) (suppress, shouldn't crash)
/// - Other errors → Err
pub async fn safe_send_message(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    text: impl Into<String>,
    keyboard: Option<InlineKeyboardMarkup>,
) -> BotHandlerInternal {
    let text = text.into();
    let mut request = bot.send_message(chat_id, &text);

    if let Some(keyboard) = keyboard {
        request = request.reply_markup(keyboard);
    }

    match request.send().await {
        Ok(_) => Ok(()),
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage
            | ApiError::MessageTextIsEmpty => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely send a message with HTML parse mode, handling common Telegram API errors.
///
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - `MessageTextIsEmpty` → Ok(()) (suppress, shouldn't crash)
/// - Other errors → Err
pub async fn safe_send_message_html(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    text: impl Into<String>,
    keyboard: Option<InlineKeyboardMarkup>,
) -> BotHandlerInternal {
    let text = text.into();
    let mut request = bot.send_message(chat_id, &text).parse_mode(ParseMode::Html);

    if let Some(keyboard) = keyboard {
        request = request.reply_markup(keyboard);
    }

    match request.send().await {
        Ok(_) => Ok(()),
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage
            | ApiError::MessageTextIsEmpty => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely edit message text with HTML parse mode, handling common Telegram API errors.
///
/// Same error handling as `safe_edit_message_text`, but sets HTML parse mode.
pub async fn safe_edit_message_text_html(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    text: impl Into<String>,
    keyboard: Option<InlineKeyboardMarkup>,
) -> BotHandlerInternal {
    let text = text.into();
    let mut request = bot
        .edit_message_text(chat_id, message_id)
        .text(text.clone())
        .parse_mode(ParseMode::Html);

    if let Some(ref keyboard) = keyboard {
        request = request.reply_markup(keyboard.clone());
    }

    match request.send().await {
        Ok(_) => Ok(()),
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::MessageNotModified => Ok(()),
            ApiError::MessageToEditNotFound | ApiError::MessageIdInvalid => {
                let mut send_request = bot.send_message(chat_id, &text).parse_mode(ParseMode::Html);
                if let Some(keyboard) = keyboard {
                    send_request = send_request.reply_markup(keyboard);
                }
                match send_request.send().await {
                    Ok(_) => Ok(()),
                    Err(RequestError::Api(
                        ApiError::NotEnoughRightsToPostMessages
                        | ApiError::NotEnoughRightsToRestrict
                        | ApiError::NotEnoughRightsToChangeChatPermissions
                        | ApiError::NotEnoughRightsToManagePins
                        | ApiError::NotEnoughRightsToPinMessage
                        | ApiError::MessageTextIsEmpty,
                    )) => Ok(()),
                    Err(e) => Err(e.into()),
                }
            }
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage
            | ApiError::MessageTextIsEmpty => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely edit message text with HTML parse mode, handling common Telegram
/// API errors, with a plain-text fallback.
///
/// Same as `safe_edit_message_text_html`, but if Telegram rejects the HTML
/// due to malformed entities (`ApiError::CantParseEntities`), retries the
/// same edit using `plain_fallback` with no parse mode, so the user still
/// gets something instead of nothing.
pub async fn safe_edit_message_text_html_with_fallback(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    html_text: impl Into<String>,
    plain_fallback: impl Into<String>,
    keyboard: Option<InlineKeyboardMarkup>,
) -> BotHandlerInternal {
    let text = html_text.into();
    let plain_fallback = plain_fallback.into();
    let mut request = bot
        .edit_message_text(chat_id, message_id)
        .text(text.clone())
        .parse_mode(ParseMode::Html);

    if let Some(ref keyboard) = keyboard {
        request = request.reply_markup(keyboard.clone());
    }

    match request.send().await {
        Ok(_) => Ok(()),
        Err(RequestError::Api(ApiError::CantParseEntities(_))) => {
            // Malformed HTML entities slipped through: retry as plain text.
            let mut plain_request = bot
                .edit_message_text(chat_id, message_id)
                .text(plain_fallback.clone());
            if let Some(ref keyboard) = keyboard {
                plain_request = plain_request.reply_markup(keyboard.clone());
            }
            match plain_request.send().await {
                Ok(_) => Ok(()),
                Err(RequestError::Api(api_error)) => match api_error {
                    ApiError::MessageNotModified => Ok(()),
                    ApiError::MessageToEditNotFound | ApiError::MessageIdInvalid => {
                        let mut send_request = bot.send_message(chat_id, &plain_fallback);
                        if let Some(keyboard) = keyboard {
                            send_request = send_request.reply_markup(keyboard);
                        }
                        match send_request.send().await {
                            Ok(_) => Ok(()),
                            Err(RequestError::Api(
                                ApiError::NotEnoughRightsToPostMessages
                                | ApiError::NotEnoughRightsToRestrict
                                | ApiError::NotEnoughRightsToChangeChatPermissions
                                | ApiError::NotEnoughRightsToManagePins
                                | ApiError::NotEnoughRightsToPinMessage
                                | ApiError::MessageTextIsEmpty,
                            )) => Ok(()),
                            Err(e) => Err(e.into()),
                        }
                    }
                    ApiError::NotEnoughRightsToPostMessages
                    | ApiError::NotEnoughRightsToRestrict
                    | ApiError::NotEnoughRightsToChangeChatPermissions
                    | ApiError::NotEnoughRightsToManagePins
                    | ApiError::NotEnoughRightsToPinMessage
                    | ApiError::MessageTextIsEmpty => Ok(()),
                    other => Err(RequestError::Api(other).into()),
                },
                Err(e) => Err(e.into()),
            }
        }
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::MessageNotModified => Ok(()),
            ApiError::MessageToEditNotFound | ApiError::MessageIdInvalid => {
                let mut send_request = bot.send_message(chat_id, &text).parse_mode(ParseMode::Html);
                if let Some(keyboard) = keyboard {
                    send_request = send_request.reply_markup(keyboard);
                }
                match send_request.send().await {
                    Ok(_) => Ok(()),
                    Err(RequestError::Api(
                        ApiError::NotEnoughRightsToPostMessages
                        | ApiError::NotEnoughRightsToRestrict
                        | ApiError::NotEnoughRightsToChangeChatPermissions
                        | ApiError::NotEnoughRightsToManagePins
                        | ApiError::NotEnoughRightsToPinMessage
                        | ApiError::MessageTextIsEmpty,
                    )) => Ok(()),
                    Err(e) => Err(e.into()),
                }
            }
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage
            | ApiError::MessageTextIsEmpty => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely send a document, handling common Telegram API errors.
///
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - Other errors → Err
pub async fn safe_send_document(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    document: InputFile,
    caption: impl Into<String>,
) -> BotHandlerInternal {
    match bot
        .send_document(chat_id, document)
        .caption(caption)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely delete a message, handling common Telegram API errors.
///
/// - `MessageToDeleteNotFound` → Ok(()) (message already deleted)
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - Other errors → Err
pub async fn safe_delete_message(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
) -> BotHandlerInternal {
    match bot.delete_message(chat_id, message_id).await {
        Ok(_) => Ok(()),
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::MessageToDeleteNotFound
            | ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely answer a callback query, suppressing all errors.
///
/// Callback query responses are non-critical UX hints. If they fail
/// (e.g., the query is too old), there's nothing actionable to do.
pub async fn safe_answer_callback_query(
    bot: &CacheMe<Throttle<Bot>>,
    callback_query_id: CallbackQueryId,
) -> BotHandlerInternal {
    match bot.answer_callback_query(callback_query_id).send().await {
        Ok(_) => Ok(()),
        Err(e) => {
            log::warn!("Failed to answer callback query: {:?}", e);
            Ok(())
        }
    }
}

/// Safely answer a callback query with text and optional alert, suppressing all errors.
///
/// Same as `safe_answer_callback_query` but supports text and alert parameters.
pub async fn safe_answer_callback_query_with_text(
    bot: &CacheMe<Throttle<Bot>>,
    callback_query_id: CallbackQueryId,
    text: &str,
    show_alert: bool,
) -> BotHandlerInternal {
    match bot
        .answer_callback_query(callback_query_id)
        .text(text)
        .show_alert(show_alert)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            log::warn!("Failed to answer callback query: {:?}", e);
            Ok(())
        }
    }
}

/// Safely copy a message, handling common Telegram API errors.
///
/// - `MessageToCopyNotFound` → Ok(()) (original message deleted)
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - Other errors → Err
pub async fn safe_copy_message(
    bot: &CacheMe<Throttle<Bot>>,
    from_chat_id: ChatId,
    to_chat_id: ChatId,
    message_id: MessageId,
) -> BotHandlerInternal {
    match bot
        .copy_message(to_chat_id, from_chat_id, message_id)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::MessageToCopyNotFound
            | ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely send a photo, handling common Telegram API errors.
///
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - Other errors → Err
pub async fn safe_send_photo(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    photo: InputFile,
) -> BotHandlerInternal {
    match bot.send_photo(chat_id, photo).send().await {
        Ok(_) => Ok(()),
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Safely send a message with reply parameters, handling common Telegram API errors.
///
/// - `MessageToReplyNotFound` → retry without reply parameters (original message was deleted)
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - `MessageTextIsEmpty` → Ok(()) (suppress, shouldn't crash)
/// - Other errors → Err
pub async fn safe_send_message_with_reply(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    text: impl Into<String>,
    reply_parameters: ReplyParameters,
    keyboard: Option<InlineKeyboardMarkup>,
) -> BotHandlerInternal {
    let text = text.into();
    let mut request = bot
        .send_message(chat_id, &text)
        .reply_parameters(reply_parameters);

    if let Some(ref keyboard) = keyboard {
        request = request.reply_markup(keyboard.clone());
    }

    match request.send().await {
        Ok(_) => Ok(()),
        Err(RequestError::Api(ApiError::MessageToReplyNotFound)) => {
            // Original message was deleted, send without reply
            let mut fallback = bot.send_message(chat_id, &text);
            if let Some(keyboard) = keyboard {
                fallback = fallback.reply_markup(keyboard);
            }
            match fallback.send().await {
                Ok(_) => Ok(()),
                Err(RequestError::Api(
                    ApiError::NotEnoughRightsToPostMessages
                    | ApiError::NotEnoughRightsToRestrict
                    | ApiError::NotEnoughRightsToChangeChatPermissions
                    | ApiError::NotEnoughRightsToManagePins
                    | ApiError::NotEnoughRightsToPinMessage
                    | ApiError::MessageTextIsEmpty,
                )) => Ok(()),
                Err(e) => Err(e.into()),
            }
        }
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage
            | ApiError::MessageTextIsEmpty => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}

/// Attempt to send a whole annotation (or other long content) as a single
/// Telegram "Rich Message" (`sendRichMessage`, Bot API 10.1+), using the
/// structured `blocks` field (never the `html`/`markdown` fields -- those
/// were found via live testing to *silently truncate* oversized content
/// while still returning HTTP 200; `blocks` either preserves content
/// byte-for-byte or rejects the whole request with an explicit error).
///
/// `skip_entity_detection` is always set so Telegram's own automatic
/// entity detection doesn't second-guess the explicit formatting decisions
/// already baked into `blocks` by the tokenizer/renderer.
///
/// This is intentionally *not* folded into the `BotHandlerInternal`-returning
/// `safe_*` family: the caller needs to distinguish "nothing more to do"
/// from "this didn't work, fall back to the paginated `send_message` flow",
/// which a plain `Ok(())`/`Err` can't express on its own.
///
/// - `Ok(true)` → sent successfully, caller is done.
/// - `Ok(false)` → a known, safe-to-ignore Telegram error occurred (missing
///   rights, empty text). Nothing more can be done; caller should stop, not
///   fall back to pagination.
/// - `Err(_)` → any other error, including ones indicating the Bot API
///   server in use doesn't support `sendRichMessage` at all, or the content
///   exceeded the (empirically-verified) safe size budget (e.g.
///   `RICH_MESSAGE_TEXT_TOO_LONG` / `RICH_MESSAGE_TOO_LARGE`). Caller should
///   fall back to the existing paginated `send_message` approach.
///
/// `MessageToReplyNotFound` is retried once without `reply_parameters`,
/// mirroring `safe_send_message_with_reply`.
/// Note: takes an additional `keyboard` parameter beyond what the original
/// `html`-based version of this function had -- necessary because unlike
/// the old design (which rendered a whole annotation into a single
/// unpaginated Rich Message), the new `blocks`-based design still paginates
/// (via [`super::super::annotations::pages::build_rich_pages`]), so the
/// prev/next inline keyboard needs to be attachable to the initial send,
/// exactly like the existing plain-text `safe_send_message_with_reply_html`
/// path already supports.
pub async fn safe_send_rich_message_with_reply(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    blocks: Vec<InputRichBlock>,
    reply_parameters: ReplyParameters,
    keyboard: Option<InlineKeyboardMarkup>,
) -> anyhow::Result<bool> {
    let rich_message = InputRichMessage {
        html: None,
        markdown: None,
        blocks: Some(blocks),
        media: None,
        is_rtl: None,
        skip_entity_detection: Some(true),
    };

    let mut request = bot
        .send_rich_message(chat_id, rich_message.clone())
        .reply_parameters(reply_parameters);
    if let Some(ref keyboard) = keyboard {
        request = request.reply_markup(keyboard.clone());
    }

    match request.send().await {
        Ok(_) => Ok(true),
        Err(RequestError::Api(ApiError::MessageToReplyNotFound)) => {
            // Original message was deleted, send without reply.
            let mut fallback = bot.send_rich_message(chat_id, rich_message);
            if let Some(keyboard) = keyboard {
                fallback = fallback.reply_markup(keyboard);
            }
            match fallback.send().await {
                Ok(_) => Ok(true),
                Err(RequestError::Api(
                    ApiError::NotEnoughRightsToPostMessages
                    | ApiError::NotEnoughRightsToRestrict
                    | ApiError::NotEnoughRightsToChangeChatPermissions
                    | ApiError::NotEnoughRightsToManagePins
                    | ApiError::NotEnoughRightsToPinMessage
                    | ApiError::MessageTextIsEmpty,
                )) => Ok(false),
                Err(e) => Err(e.into()),
            }
        }
        Err(RequestError::Api(
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage
            | ApiError::MessageTextIsEmpty,
        )) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Edit-in-place counterpart to [`safe_send_rich_message_with_reply`], for
/// the prev/next pagination flow on messages that were originally sent as a
/// Rich Message.
///
/// Same `Ok`/`Err` contract as [`safe_send_rich_message_with_reply`]:
/// `Ok(true)`/`Ok(false)` mean "done, nothing more for the caller to do";
/// `Err(_)` means the caller should fall back to the plain-text pagination
/// path instead.
pub async fn safe_edit_rich_message_text(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    blocks: Vec<InputRichBlock>,
    keyboard: Option<InlineKeyboardMarkup>,
) -> anyhow::Result<bool> {
    let rich_message = InputRichMessage {
        html: None,
        markdown: None,
        blocks: Some(blocks),
        media: None,
        is_rtl: None,
        skip_entity_detection: Some(true),
    };

    let mut request = bot
        .edit_message_text(chat_id, message_id)
        .rich_message(rich_message.clone());

    if let Some(ref keyboard) = keyboard {
        request = request.reply_markup(keyboard.clone());
    }

    match request.send().await {
        Ok(_) => Ok(true),
        Err(RequestError::Api(ApiError::MessageToEditNotFound | ApiError::MessageIdInvalid)) => {
            // Original message was deleted, send as new message.
            let mut send_request = bot.send_rich_message(chat_id, rich_message);
            if let Some(keyboard) = keyboard {
                send_request = send_request.reply_markup(keyboard);
            }
            match send_request.send().await {
                Ok(_) => Ok(true),
                Err(RequestError::Api(
                    ApiError::NotEnoughRightsToPostMessages
                    | ApiError::NotEnoughRightsToRestrict
                    | ApiError::NotEnoughRightsToChangeChatPermissions
                    | ApiError::NotEnoughRightsToManagePins
                    | ApiError::NotEnoughRightsToPinMessage
                    | ApiError::MessageTextIsEmpty,
                )) => Ok(false),
                Err(e) => Err(e.into()),
            }
        }
        Err(RequestError::Api(
            ApiError::MessageNotModified
            | ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage
            | ApiError::MessageTextIsEmpty,
        )) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// Safely send a message with reply parameters and HTML parse mode,
/// handling common Telegram API errors.
///
/// Same error handling as `safe_send_message_with_reply`, but sets HTML
/// parse mode on both the primary send and the `MessageToReplyNotFound`
/// fallback send. If Telegram rejects the HTML due to malformed entities
/// (`ApiError::CantParseEntities`), retries the same send using
/// `plain_fallback` with no parse mode, so the user still gets something
/// instead of nothing.
///
/// - `MessageToReplyNotFound` → retry without reply parameters (original message was deleted)
/// - `CantParseEntities` → retry with `plain_fallback`, no parse mode
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - `MessageTextIsEmpty` → Ok(()) (suppress, shouldn't crash)
/// - Other errors → Err
pub async fn safe_send_message_with_reply_html(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    html_text: impl Into<String>,
    plain_fallback: impl Into<String>,
    reply_parameters: ReplyParameters,
    keyboard: Option<InlineKeyboardMarkup>,
) -> BotHandlerInternal {
    let text = html_text.into();
    let plain_fallback = plain_fallback.into();
    let mut request = bot
        .send_message(chat_id, &text)
        .parse_mode(ParseMode::Html)
        .reply_parameters(reply_parameters.clone());

    if let Some(ref keyboard) = keyboard {
        request = request.reply_markup(keyboard.clone());
    }

    match request.send().await {
        Ok(_) => Ok(()),
        Err(RequestError::Api(ApiError::CantParseEntities(_))) => {
            // Malformed HTML entities slipped through: retry as plain text.
            let mut plain_request = bot
                .send_message(chat_id, &plain_fallback)
                .reply_parameters(reply_parameters);
            if let Some(ref keyboard) = keyboard {
                plain_request = plain_request.reply_markup(keyboard.clone());
            }
            match plain_request.send().await {
                Ok(_) => Ok(()),
                Err(RequestError::Api(ApiError::MessageToReplyNotFound)) => {
                    let mut fallback = bot.send_message(chat_id, &plain_fallback);
                    if let Some(keyboard) = keyboard {
                        fallback = fallback.reply_markup(keyboard);
                    }
                    match fallback.send().await {
                        Ok(_) => Ok(()),
                        Err(RequestError::Api(
                            ApiError::NotEnoughRightsToPostMessages
                            | ApiError::NotEnoughRightsToRestrict
                            | ApiError::NotEnoughRightsToChangeChatPermissions
                            | ApiError::NotEnoughRightsToManagePins
                            | ApiError::NotEnoughRightsToPinMessage
                            | ApiError::MessageTextIsEmpty,
                        )) => Ok(()),
                        Err(e) => Err(e.into()),
                    }
                }
                Err(RequestError::Api(api_error)) => match api_error {
                    ApiError::NotEnoughRightsToPostMessages
                    | ApiError::NotEnoughRightsToRestrict
                    | ApiError::NotEnoughRightsToChangeChatPermissions
                    | ApiError::NotEnoughRightsToManagePins
                    | ApiError::NotEnoughRightsToPinMessage
                    | ApiError::MessageTextIsEmpty => Ok(()),
                    other => Err(RequestError::Api(other).into()),
                },
                Err(e) => Err(e.into()),
            }
        }
        Err(RequestError::Api(ApiError::MessageToReplyNotFound)) => {
            // Original message was deleted, send without reply
            let mut fallback = bot.send_message(chat_id, &text).parse_mode(ParseMode::Html);
            if let Some(ref keyboard) = keyboard {
                fallback = fallback.reply_markup(keyboard.clone());
            }
            match fallback.send().await {
                Ok(_) => Ok(()),
                Err(RequestError::Api(ApiError::CantParseEntities(_))) => {
                    let mut plain_fallback_request = bot.send_message(chat_id, &plain_fallback);
                    if let Some(keyboard) = keyboard {
                        plain_fallback_request = plain_fallback_request.reply_markup(keyboard);
                    }
                    match plain_fallback_request.send().await {
                        Ok(_) => Ok(()),
                        Err(RequestError::Api(
                            ApiError::NotEnoughRightsToPostMessages
                            | ApiError::NotEnoughRightsToRestrict
                            | ApiError::NotEnoughRightsToChangeChatPermissions
                            | ApiError::NotEnoughRightsToManagePins
                            | ApiError::NotEnoughRightsToPinMessage
                            | ApiError::MessageTextIsEmpty,
                        )) => Ok(()),
                        Err(e) => Err(e.into()),
                    }
                }
                Err(RequestError::Api(
                    ApiError::NotEnoughRightsToPostMessages
                    | ApiError::NotEnoughRightsToRestrict
                    | ApiError::NotEnoughRightsToChangeChatPermissions
                    | ApiError::NotEnoughRightsToManagePins
                    | ApiError::NotEnoughRightsToPinMessage
                    | ApiError::MessageTextIsEmpty,
                )) => Ok(()),
                Err(e) => Err(e.into()),
            }
        }
        Err(RequestError::Api(api_error)) => match api_error {
            ApiError::NotEnoughRightsToPostMessages
            | ApiError::NotEnoughRightsToRestrict
            | ApiError::NotEnoughRightsToChangeChatPermissions
            | ApiError::NotEnoughRightsToManagePins
            | ApiError::NotEnoughRightsToPinMessage
            | ApiError::MessageTextIsEmpty => Ok(()),
            other => Err(RequestError::Api(other).into()),
        },
        Err(e) => Err(e.into()),
    }
}
