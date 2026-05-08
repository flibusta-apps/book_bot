use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{
        CallbackQueryId, InlineKeyboardMarkup, InputFile, MessageId, ParseMode, ReplyParameters,
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
    let mut request = bot.edit_message_text(chat_id, message_id, &text);

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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub async fn safe_edit_message_text_html(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    text: impl Into<String>,
    keyboard: Option<InlineKeyboardMarkup>,
) -> BotHandlerInternal {
    let text = text.into();
    let mut request = bot
        .edit_message_text(chat_id, message_id, &text)
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

/// Safely send a document, handling common Telegram API errors.
///
/// - `NotEnoughRights*` → Ok(()) (can't act, suppress)
/// - Other errors → Err
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
