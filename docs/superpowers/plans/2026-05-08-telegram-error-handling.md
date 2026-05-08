# Telegram Error Handling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate ~12000+ Sentry ERROR events from expected Telegram API errors by handling them gracefully at both the error handler and handler levels.

**Architecture:** Two-layer fix: (1) update `custom_error_handler.rs` to classify expected Telegram errors as WARN/DEBUG instead of ERROR, (2) add new safe_* wrapper functions in `telegram_utils.rs` and replace all raw Telegram API calls in handlers with safe wrappers.

**Tech Stack:** Rust, teloxide 0.17.0, anyhow

---

### Task 1: Update `classify_error()` in `custom_error_handler.rs`

**Files:**
- Modify: `book_bot/src/bots_manager/custom_error_handler.rs`

- [ ] **Step 1: Add Telegram message error patterns to `classify_error()`**

Add a new classification block after the existing "Telegram permission/authorization errors" block (after line 59, before `log::Level::Error`):

```rust
    // Telegram message errors — expected in production when messages are
    // deleted between requests, users click the same button, etc.
    if error_string.contains("message to edit not found")
        || error_string.contains("message is not modified")
        || error_string.contains("MESSAGE_ID_INVALID")
        || error_string.contains("text must be non-empty")
    {
        return log::Level::Warn;
    }
```

- [ ] **Step 2: Add early return for `message is not modified` in `handle_error()`**

Add after the existing `message to be replied not found` early return (after line 78):

```rust
            if error_string.contains("message is not modified") {
                log::debug!("Ignoring Telegram not-modified error: {:?}", error);
                return;
            }
```

- [ ] **Step 3: Run `cargo check` to verify compilation**

Run: `cd book_bot && cargo check`
Expected: Compiles without errors

- [ ] **Step 4: Commit**

```bash
cd book_bot && git add book_bot/src/bots_manager/custom_error_handler.rs && git commit -m "fix: classify expected Telegram message errors as WARN in error handler

Previously these errors were logged at ERROR level and sent to Sentry:
- message to edit not found (BOOK-BOT-15P, 5626 events)
- message is not modified (BOOK-BOT-15J, 6120 events)
- MESSAGE_ID_INVALID (BOOK-BOT-15Q)
- text must be non-empty (BOOK-BOT-15Z, 283 events)

These are normal operational conditions, not bugs."
```

---

### Task 2: Add `safe_send_document`, `safe_delete_message`, `safe_answer_callback_query` to `telegram_utils.rs`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/utils/telegram_utils.rs`

- [ ] **Step 1: Add `safe_send_document` function**

Add after `safe_send_message_with_reply` (after line 187). This function wraps `bot.send_document()` to handle permission errors gracefully:

```rust
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
```

- [ ] **Step 2: Add `safe_delete_message` function**

Add after `safe_send_document`:

```rust
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
```

- [ ] **Step 3: Add `safe_answer_callback_query` function**

Add after `safe_delete_message`:

```rust
/// Safely answer a callback query, suppressing all errors.
///
/// Callback query responses are non-critical UX hints. If they fail
/// (e.g., the query is too old), there's nothing actionable to do.
pub async fn safe_answer_callback_query(
    bot: &CacheMe<Throttle<Bot>>,
    callback_query_id: String,
) -> BotHandlerInternal {
    match bot.answer_callback_query(callback_query_id).send().await {
        Ok(_) => Ok(()),
        Err(e) => {
            log::warn!("Failed to answer callback query: {:?}", e);
            Ok(())
        }
    }
}
```

- [ ] **Step 4: Add `safe_answer_callback_query_with_text` function for callback queries with text/alert**

Add after `safe_answer_callback_query`:

```rust
/// Safely answer a callback query with text and optional alert, suppressing all errors.
///
/// Same as `safe_answer_callback_query` but supports text and alert parameters.
pub async fn safe_answer_callback_query_with_text(
    bot: &CacheMe<Throttle<Bot>>,
    callback_query_id: String,
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
```

- [ ] **Step 5: Add `InputFile` to imports**

Update the imports at the top of the file (line 1-6) to include `InputFile`:

```rust
use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InlineKeyboardMarkup, InputFile, MessageId, ReplyParameters},
    ApiError, RequestError,
};
```

- [ ] **Step 6: Run `cargo check` to verify compilation**

Run: `cd book_bot && cargo check`
Expected: Compiles without errors

- [ ] **Step 7: Commit**

```bash
cd book_bot && git add book_bot/src/bots/approved_bot/modules/utils/telegram_utils.rs && git commit -m "feat: add safe_send_document, safe_delete_message, safe_answer_callback_query wrappers

New safe_* functions for graceful Telegram API error handling:
- safe_send_document: suppresses NotEnoughRights errors
- safe_delete_message: suppresses MessageToDeleteNotFound and NotEnoughRights
- safe_answer_callback_query: suppresses all errors (non-critical)
- safe_answer_callback_query_with_text: same with text/alert support"
```

---

### Task 3: Replace raw API calls in `download/mod.rs`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/download/mod.rs`

This file has the most raw API calls. Replace them with safe_* wrappers.

- [ ] **Step 1: Add imports for new safe_* functions**

Add to the existing import from `super::utils::telegram_utils` on line 5:

```rust
use super::utils::telegram_utils::{
    safe_edit_message_text, safe_send_message_with_reply, safe_send_document, safe_delete_message,
};
```

- [ ] **Step 2: Replace `bot.send_message(message.chat.id, NOT_FOUND).send().await?` on line 200**

Change:
```rust
            bot.send_message(message.chat.id, NOT_FOUND).send().await?;
```
To:
```rust
            safe_send_message_with_reply(&bot, message.chat.id, NOT_FOUND, ReplyParameters::new(message.id), None).await?;
```

Add `ReplyParameters` to imports from teloxide if not already there.

- [ ] **Step 3: Replace `bot.send_message(message.chat.id, ERROR_TRY_LATER).send().await?` on lines 204-205**

Change:
```rust
            bot.send_message(message.chat.id, ERROR_TRY_LATER)
                .send()
                .await?;
```
To:
```rust
            safe_send_message_with_reply(&bot, message.chat.id, ERROR_TRY_LATER, ReplyParameters::new(message.id), None).await?;
```

- [ ] **Step 4: Replace `bot.send_message(message.chat.id, NOT_FOUND).send().await?` on line 269**

Change:
```rust
            bot.send_message(message.chat.id, NOT_FOUND).send().await?;
```
To:
```rust
            safe_send_message_with_reply(&bot, message.chat.id, NOT_FOUND, ReplyParameters::new(message.id), None).await?;
```

- [ ] **Step 5: Replace `bot.edit_message_text(...)` in `send_archive_link` (lines 345-357)**

Change:
```rust
    bot.edit_message_text(
        chat_id,
        message_id,
        format!(
            "Файл не может быть загружен в чат! \n \
                    Вы можете скачать его <a href=\"{link}\">по ссылке</a> (работает 3 часа)"
        ),
    )
    .parse_mode(ParseMode::Html)
    .reply_markup(InlineKeyboardMarkup {
        inline_keyboard: vec![],
    })
    .await?;
```
To:
```rust
    safe_edit_message_text(
        &bot,
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
```

Note: `safe_edit_message_text` doesn't set `parse_mode(ParseMode::Html)`. We need to update `safe_edit_message_text` to support HTML parse mode, OR we need to add a new variant. Since this is the only place that uses HTML parse mode with edit_message_text, the simplest approach is to add a `safe_edit_message_text_html` function. But to keep changes minimal, let's check if the current `safe_edit_message_text` already handles this. It doesn't set parse_mode. We need to add parse_mode support.

**Revised approach:** Add an optional `parse_mode` parameter to `safe_edit_message_text`. But that changes the signature and all callers. Instead, add a separate `safe_edit_message_text_html` function.

Actually, looking more carefully, the simplest approach is to add a `safe_edit_message_text_with_parse_mode` function that takes a `ParseMode` parameter. But this adds complexity. Let me reconsider.

The current `send_archive_link` uses `.parse_mode(ParseMode::Html)` because the message contains an `<a href>` tag. If we use `safe_edit_message_text` without HTML parse mode, the link won't render properly.

**Decision:** Add a new `safe_edit_message_text_html` wrapper that sets `ParseMode::Html`:

```rust
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
                let mut send_request = bot
                    .send_message(chat_id, &text)
                    .parse_mode(ParseMode::Html);
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
```

Add `ParseMode` to the imports in `telegram_utils.rs`:
```rust
use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InlineKeyboardMarkup, InputFile, MessageId, ParseMode, ReplyParameters},
    ApiError, RequestError,
};
```

- [ ] **Step 6: Replace `bot.delete_message(...)` calls with `safe_delete_message`**

Line 105: Change `let _ = bot.delete_message(message.chat.id, message.id).await;` to:
```rust
let _ = safe_delete_message(&bot, message.chat.id, message.id).await;
```

Line 167: Change `let _ = bot.delete_message(message.chat.id, message.id).await;` to:
```rust
let _ = safe_delete_message(&bot, message.chat.id, message.id).await;
```

Line 460: Change `let _ = bot.delete_message(message.chat.id, message.id).await;` to:
```rust
let _ = safe_delete_message(&bot, message.chat.id, message.id).await;
```

- [ ] **Step 7: Replace `bot.send_document(...)` in `_send_downloaded_file` (lines 140-143)**

Change:
```rust
    bot.send_document(message.chat().id, document)
        .caption(caption)
        .send()
        .await?;
```
To:
```rust
    safe_send_document(&bot, message.chat().id, document, caption).await?;
```

- [ ] **Step 8: Add `ReplyParameters` to teloxide imports in download/mod.rs**

The `download/mod.rs` file already imports `ReplyParameters` on line 4. Verify it's there and add if missing.

- [ ] **Step 9: Run `cargo check` to verify compilation**

Run: `cd book_bot && cargo check`
Expected: Compiles without errors

- [ ] **Step 10: Commit**

```bash
cd book_bot && git add book_bot/src/bots/approved_bot/modules/download/mod.rs book_bot/src/bots/approved_bot/modules/utils/telegram_utils.rs && git commit -m "fix: replace raw Telegram API calls with safe wrappers in download module

- Use safe_send_message_with_reply instead of bot.send_message
- Use safe_edit_message_text_html for archive link
- Use safe_delete_message instead of bot.delete_message
- Use safe_send_document instead of bot.send_document
- Add safe_edit_message_text_html to telegram_utils.rs"
```

---

### Task 4: Replace raw API calls in `book/mod.rs`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/book/mod.rs`

- [ ] **Step 1: Add imports for safe_* functions**

Add to imports:
```rust
use super::utils::telegram_utils::{safe_edit_message_text, safe_send_message_with_reply};
```

Also add `ReplyParameters` to the teloxide import if not present. Currently line 13-18 imports from teloxide. Add `ReplyParameters` to the types import.

- [ ] **Step 2: Replace raw `bot.send_message` calls in `send_book_handler`**

Line 63-66: Change:
```rust
            return match bot.send_message(chat_id, REPEAT_REQUEST).send().await {
                Ok(_) => Ok(()),
                Err(err) => Err(err.into()),
            }
```
To:
```rust
            return safe_send_message_with_reply(&bot, chat_id, REPEAT_REQUEST, ReplyParameters::new(message.id), None).await;
```

Line 75: Change:
```rust
            bot.send_message(chat_id, NOT_FOUND).send().await?;
```
To:
```rust
            safe_send_message_with_reply(&bot, chat_id, NOT_FOUND, ReplyParameters::new(message.id), None).await?;
```

Line 79: Change:
```rust
            bot.send_message(chat_id, ERROR_TRY_LATER).send().await?;
```
To:
```rust
            safe_send_message_with_reply(&bot, chat_id, ERROR_TRY_LATER, ReplyParameters::new(message.id), None).await?;
```

Line 85: Change:
```rust
        bot.send_message(chat_id, BOOKS_NOT_FOUND).send().await?;
```
To:
```rust
        safe_send_message_with_reply(&bot, chat_id, BOOKS_NOT_FOUND, ReplyParameters::new(message.id), None).await?;
```

Lines 99-102: Change:
```rust
    bot.send_message(chat_id, formatted_page)
        .reply_markup(keyboard)
        .send()
        .await?;
```
To:
```rust
    safe_send_message_with_reply(&bot, chat_id, formatted_page, ReplyParameters::new(message.id), Some(keyboard)).await?;
```

- [ ] **Step 3: Replace raw `bot.send_message` calls in `send_pagination_book_handler`**

Line 132: Change:
```rust
            bot.send_message(chat_id, REPEAT_SEARCH).send().await?;
```
To:
```rust
            safe_send_message_with_reply(&bot, chat_id, REPEAT_SEARCH, ReplyParameters::new(message.id), None).await?;
```

Wait — line 132 is inside `send_pagination_book_handler` which handles `CallbackQuery`, not `Message`. The `message` here is from `cq.message`. Let me re-read the code.

Looking at lines 125-138:
```rust
    let chat_id = cq.message.as_ref().map(|message| message.chat().id);
    let user_id = cq.from.id;
    let message_id = cq.message.as_ref().map(|message| message.id());

    let (chat_id, message_id) = match (chat_id, message_id) {
        (Some(chat_id), Some(message_id)) => (chat_id, message_id),
        (Some(chat_id), None) => {
            bot.send_message(chat_id, REPEAT_SEARCH).send().await?;
            return Ok(());
        }
        _ => {
            return Ok(());
        }
    };
```

Here `chat_id` is available but we don't have a message to reply to. Use `safe_send_message` (without reply):

```rust
            crate::bots::approved_bot::modules::utils::telegram_utils::safe_send_message(&bot, chat_id, REPEAT_SEARCH, None).await?;
```

Actually, let me check if `safe_send_message` is imported. It's currently marked `#[allow(dead_code)]` in telegram_utils.rs, so it exists but isn't used yet. We need to import it.

Lines 145-148: Change:
```rust
            match bot.send_message(chat_id, NOT_FOUND).send().await {
                Ok(_) => (),
                Err(err) => log::error!("{err:?}"),
            }
```
To:
```rust
            match safe_send_message(&bot, chat_id, NOT_FOUND, None).await {
                Ok(_) => (),
                Err(err) => log::error!("{err:?}"),
            }
```

Lines 152-155: Change:
```rust
            match bot.send_message(chat_id, ERROR_TRY_LATER).send().await {
                Ok(_) => (),
                Err(err) => log::error!("{err:?}"),
            }
```
To:
```rust
            match safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await {
                Ok(_) => (),
                Err(err) => log::error!("{err:?}"),
            }
```

Line 161: Change:
```rust
        bot.send_message(chat_id, BOOKS_NOT_FOUND).send().await?;
```
To:
```rust
        safe_send_message(&bot, chat_id, BOOKS_NOT_FOUND, None).await?;
```

Lines 169: Change:
```rust
                bot.send_message(chat_id, NOT_FOUND).send().await?;
```
To:
```rust
                safe_send_message(&bot, chat_id, NOT_FOUND, None).await?;
```

Lines 173-176: Change:
```rust
                bot.send_message(chat_id, ERROR_TRY_LATER).send().await?;
```
To:
```rust
                safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
```

- [ ] **Step 4: Update imports in book/mod.rs**

Add `safe_send_message` to the import from `telegram_utils`:
```rust
use super::utils::telegram_utils::{safe_edit_message_text, safe_send_message, safe_send_message_with_reply};
```

Add `ReplyParameters` to teloxide imports.

- [ ] **Step 5: Run `cargo check` to verify compilation**

Run: `cd book_bot && cargo check`
Expected: Compiles without errors

- [ ] **Step 6: Commit**

```bash
cd book_bot && git add book_bot/src/bots/approved_bot/modules/book/mod.rs && git commit -m "fix: replace raw Telegram API calls with safe wrappers in book module"
```

---

### Task 5: Replace raw API calls in `search/mod.rs`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/search/mod.rs`

- [ ] **Step 1: Add `safe_send_message` to imports**

Current import on line 22-24:
```rust
use crate::bots::approved_bot::modules::utils::telegram_utils::{
    safe_edit_message_text, safe_send_message_with_reply,
};
```
Change to:
```rust
use crate::bots::approved_bot::modules::utils::telegram_utils::{
    safe_edit_message_text, safe_send_message, safe_send_message_with_reply,
};
```

- [ ] **Step 2: Replace raw `bot.send_message` calls in `generic_search_pagination_handler`**

Line 65: Change:
```rust
            bot.send_message(chat_id, REPEAT_SEARCH).send().await?;
```
To:
```rust
            safe_send_message(&bot, chat_id, REPEAT_SEARCH, None).await?;
```

Line 92: Change:
```rust
            bot.send_message(chat_id, message_text).send().await?;
```
To:
```rust
            safe_send_message(&bot, chat_id, message_text, None).await?;
```

Line 96: Change:
```rust
            bot.send_message(chat_id, ERROR_TRY_LATER).send().await?;
```
To:
```rust
            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
```

Line 110: Change:
```rust
        bot.send_message(chat_id, message_text).send().await?;
```
To:
```rust
        safe_send_message(&bot, chat_id, message_text, None).await?;
```

Lines 118, 122: Change:
```rust
                bot.send_message(chat_id, ERROR_TRY_LATER).send().await?;
```
To:
```rust
                safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
```

- [ ] **Step 3: Replace raw `bot.send_message` calls in `message_handler`**

Lines 177, 207, 239, 270: These are inside `Err(_)` branches that currently do:
```rust
                        Err(_) => {
                            bot.send_message(chat_id, ERROR_TRY_LATER).send().await?;
                            return Ok(());
                        }
```
Change each to:
```rust
                        Err(_) => {
                            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                            return Ok(());
                        }
```

- [ ] **Step 4: Run `cargo check` to verify compilation**

Run: `cd book_bot && cargo check`
Expected: Compiles without errors

- [ ] **Step 5: Commit**

```bash
cd book_bot && git add book_bot/src/bots/approved_bot/modules/search/mod.rs && git commit -m "fix: replace raw Telegram API calls with safe wrappers in search module"
```

---

### Task 6: Replace raw API calls in `annotations/mod.rs`, `settings/mod.rs`, `support/mod.rs`, `update_history/mod.rs`, `random/mod.rs`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/annotations/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/settings/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/support/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/update_history/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/random/mod.rs`

- [ ] **Step 1: Update `annotations/mod.rs`**

Add import for `safe_send_message_with_reply`:
```rust
use crate::bots::approved_bot::modules::utils::telegram_utils::{
    safe_edit_message_text, safe_send_message_with_reply,
};
```

Replace line 97-99 (the `bot.send_photo` block is fine — it's not a text message, and `send_photo` errors are different). But line 123-126:
```rust
    bot.send_message(message.chat.id, current_text)
        .reply_markup(keyboard)
        .send()
        .await?;
```
Change to:
```rust
    safe_send_message_with_reply(
        &bot,
        message.chat.id,
        current_text,
        ReplyParameters::new(message.id),
        Some(keyboard),
    )
    .await?;
```

Add `ReplyParameters` to teloxide imports.

- [ ] **Step 2: Update `settings/mod.rs`**

Add imports:
```rust
use crate::bots::approved_bot::modules::utils::telegram_utils::{
    safe_edit_message_reply_markup, safe_edit_message_text, safe_send_message,
    safe_answer_callback_query, safe_answer_callback_query_with_text,
};
```

Replace line 52-55:
```rust
    bot.send_message(message.chat.id, "Настройки")
        .reply_markup(get_main_settings_keyboard())
        .send()
        .await?;
```
To:
```rust
    safe_send_message(&bot, message.chat.id, "Настройки", Some(get_main_settings_keyboard())).await?;
```

Replace line 166-168:
```rust
            bot.send_message(cq.from.id, "Ошибка! Попробуйте заново(")
                .send()
                .await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, "Ошибка! Попробуйте заново(", None).await?;
```

Replace line 188:
```rust
            bot.answer_callback_query(cq.id).send().await?;
```
To:
```rust
            safe_answer_callback_query(&bot, cq.id).await?;
```

Replace line 200:
```rust
            bot.answer_callback_query(cq.id).send().await?;
```
To:
```rust
            safe_answer_callback_query(&bot, cq.id).await?;
```

Replace line 212:
```rust
            bot.answer_callback_query(cq.id).send().await?;
```
To:
```rust
            safe_answer_callback_query(&bot, cq.id).await?;
```

Replace lines 226:
```rust
                bot.answer_callback_query(cq.id).send().await?;
```
To:
```rust
                safe_answer_callback_query(&bot, cq.id).await?;
```

Replace lines 241-245:
```rust
                bot.answer_callback_query(cq.id)
                    .text("Ошибка! Попробуйте заново(")
                    .show_alert(true)
                    .send()
                    .await?;
```
To:
```rust
                safe_answer_callback_query_with_text(&bot, cq.id, "Ошибка! Попробуйте заново(", true).await?;
```

Replace lines 256-259:
```rust
            bot.answer_callback_query(cq.id)
                .text("Готово")
                .send()
                .await?;
```
To:
```rust
            safe_answer_callback_query_with_text(&bot, cq.id, "Готово", false).await?;
```

Replace line 287:
```rust
        bot.answer_callback_query(cq.id)
```
This is part of a longer chain. Read the full context around line 287-291:
```rust
        bot.answer_callback_query(cq.id)
            .text("Должен быть активен, хотя бы один язык!")
            .show_alert(true)
            .send()
            .await?;
```
Change to:
```rust
        safe_answer_callback_query_with_text(&bot, cq.id, "Должен быть активен, хотя бы один язык!", true).await?;
```

Replace lines 310-312:
```rust
        bot.send_message(message.chat().id, "Ошибка! Попробуйте заново(")
            .send()
            .await?;
```
To:
```rust
        safe_send_message(&bot, message.chat().id, "Ошибка! Попробуйте заново(", None).await?;
```

Replace lines 319-321:
```rust
            bot.send_message(message.chat().id, "Ошибка! Попробуйте заново(")
                .send()
                .await?;
```
To:
```rust
            safe_send_message(&bot, message.chat().id, "Ошибка! Попробуйте заново(", None).await?;
```

- [ ] **Step 3: Update `support/mod.rs`**

Add import:
```rust
use crate::bots::approved_bot::modules::utils::telegram_utils::safe_send_message;
```

Replace lines 59-61:
```rust
    bot.send_message(message.chat.id, message_text)
        .parse_mode(teloxide::types::ParseMode::Html)
        .await?;
```

This uses HTML parse mode. We need a `safe_send_message_html` function or handle this differently. Since this is the only `send_message` with HTML parse mode, add a new wrapper:

Actually, let's add `safe_send_message_html` to `telegram_utils.rs`:

```rust
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
```

Then in `support/mod.rs`, change lines 59-61 to:
```rust
    safe_send_message_html(&bot, message.chat.id, message_text, None).await?;
```

- [ ] **Step 4: Update `update_history/mod.rs`**

Add import for `safe_send_message`:
```rust
use crate::bots::approved_bot::modules::utils::telegram_utils::{safe_edit_message_text, safe_send_message};
```

Replace lines 75-83:
```rust
    match bot
        .send_message(message.chat.id, "Обновление каталога:")
        .reply_markup(keyboard)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(err.into()),
    }
```
To:
```rust
    safe_send_message(&bot, message.chat.id, "Обновление каталога:", Some(keyboard)).await
```

Replace line 95:
```rust
            bot.send_message(cq.from.id, ERROR_TRY_AGAIN).send().await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, ERROR_TRY_AGAIN, None).await?;
```

Replace lines 122-125:
```rust
            bot.send_message(message.chat().id, "Нет новых книг за этот период.")
                .send()
                .await?;
```
To:
```rust
            safe_send_message(&bot, message.chat().id, "Нет новых книг за этот период.", None).await?;
```

Replace lines 130-132:
```rust
        bot.send_message(message.chat().id, "Нет новых книг за этот период.")
            .send()
            .await?;
```
To:
```rust
        safe_send_message(&bot, message.chat().id, "Нет новых книг за этот период.", None).await?;
```

Replace lines 154-156:
```rust
                bot.send_message(message.chat().id, "Нет новых книг за этот период.")
                    .send()
                    .await?;
```
To:
```rust
                safe_send_message(&bot, message.chat().id, "Нет новых книг за этот период.", None).await?;
```

- [ ] **Step 5: Update `random/mod.rs`**

Add `safe_send_message` to imports:
```rust
use crate::bots::approved_bot::modules::utils::telegram_utils::{
    safe_edit_message_reply_markup, safe_send_message, safe_send_message_with_reply,
};
```

Replace line 90:
```rust
            bot.send_message(cq.from.id, "Не найдено :(").send().await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, "Не найдено :(", None).await?;
```

Replace lines 94-96:
```rust
            bot.send_message(cq.from.id, "Ошибка! Попробуйте позже :(")
                .send()
                .await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, "Ошибка! Попробуйте позже :(", None).await?;
```

Replace lines 103-111:
```rust
    bot.send_message(cq.from.id, item_message)
        .reply_markup(InlineKeyboardMarkup {
            inline_keyboard: vec![vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(cq.data.unwrap()),
                text: String::from("Повторить?"),
            }]],
        })
        .send()
        .await?;
```
To:
```rust
    safe_send_message(
        &bot,
        cq.from.id,
        item_message,
        Some(InlineKeyboardMarkup {
            inline_keyboard: vec![vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(cq.data.unwrap()),
                text: String::from("Повторить?"),
            }]],
        }),
    )
    .await?;
```

Replace line 155:
```rust
            bot.send_message(cq.from.id, "Не найдено :(").send().await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, "Не найдено :(", None).await?;
```

Replace lines 164-166:
```rust
            bot.send_message(cq.from.id, "Ошибка! Начните заново :(")
                .send()
                .await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, "Ошибка! Начните заново :(", None).await?;
```

Replace line 203:
```rust
            bot.send_message(cq.from.id, "Не найдено :(").send().await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, "Не найдено :(", None).await?;
```

Replace lines 212-214:
```rust
            bot.send_message(cq.from.id, "Ошибка! Попробуйте позже :(")
                .send()
                .await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, "Ошибка! Попробуйте позже :(", None).await?;
```

Replace line 223:
```rust
            bot.send_message(cq.from.id, "Не найдено :(").send().await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, "Не найдено :(", None).await?;
```

Replace lines 258-260:
```rust
            bot.send_message(cq.from.id, "Ошибка! Начните заново :(")
                .send()
                .await?;
```
To:
```rust
            safe_send_message(&bot, cq.from.id, "Ошибка! Начните заново :(", None).await?;
```

- [ ] **Step 6: Run `cargo check` to verify compilation**

Run: `cd book_bot && cargo check`
Expected: Compiles without errors

- [ ] **Step 7: Commit**

```bash
cd book_bot && git add book_bot/src/bots/approved_bot/modules/annotations/mod.rs book_bot/src/bots/approved_bot/modules/settings/mod.rs book_bot/src/bots/approved_bot/modules/support/mod.rs book_bot/src/bots/approved_bot/modules/update_history/mod.rs book_bot/src/bots/approved_bot/modules/random/mod.rs book_bot/src/bots/approved_bot/modules/utils/telegram_utils.rs && git commit -m "fix: replace raw Telegram API calls with safe wrappers in remaining modules

- annotations: use safe_send_message_with_reply
- settings: use safe_send_message, safe_answer_callback_query, safe_answer_callback_query_with_text
- support: use safe_send_message_html for HTML messages
- update_history: use safe_send_message
- random: use safe_send_message
- telegram_utils: add safe_send_message_html and safe_edit_message_text_html"
```

---

### Task 7: Final verification

- [ ] **Step 1: Run `cargo check`**

Run: `cd book_bot && cargo check`
Expected: Compiles without errors

- [ ] **Step 2: Run `cargo clippy --all-features`**

Run: `cd book_bot && cargo clippy --all-features`
Expected: No warnings (the `#[allow(dead_code)]` on `safe_send_message` should be removed since it's now used)

- [ ] **Step 3: Remove `#[allow(dead_code)]` from `safe_send_message` in `telegram_utils.rs`**

The `safe_send_message` function is now used, so remove the `#[allow(dead_code)]` attribute on line 104.

- [ ] **Step 4: Run `cargo fmt`**

Run: `cd book_bot && cargo fmt`
Expected: No changes or clean formatting

- [ ] **Step 5: Run `cargo clippy --all-features` again**

Run: `cd book_bot && cargo clippy --all-features`
Expected: No warnings

- [ ] **Step 6: Final commit**

```bash
cd book_bot && git add -A && git commit -m "chore: remove dead_code allow attribute from safe_send_message"
```