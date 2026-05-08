# Telegram Error Handling: Graceful Degradation in book-bot

## Problem

Four types of Telegram API errors generate thousands of Sentry ERROR events, even though they are expected production scenarios:

| Issue | Error | Events | Root Cause |
|-------|-------|--------|------------|
| BOOK-BOT-15J | `message is not modified` | 6120 | User clicked same button |
| BOOK-BOT-15P | `message to edit not found` | 5626 | Message deleted between requests |
| BOOK-BOT-15R/15T/18P | `not enough rights to send text/documents` | 33+ | Bot added to group without send rights |
| BOOK-BOT-15Z | `text must be non-empty` | 283 | Empty text sent to API |

These errors are not bugs — they are normal operational conditions in a Telegram bot. They should be handled gracefully, not reported as errors.

## Root Cause Analysis

The codebase has two error-handling layers with gaps between them:

### Layer 1: `custom_error_handler.rs` (teloxide dispatch error handler)

Catches errors that pass through all handlers unhandled. Currently classifies:
- Rate limits (`Retry after`, `Too Many Requests`, `Flood`) → WARN
- Network errors (dns, timeout, connection refused) → WARN
- Permission errors (`not enough rights`, `CHAT_WRITE_FORBIDDEN`, `bot was blocked`) → WARN
- `message to be replied not found` → DEBUG (ignored)

**Missing classifications** (logged as ERROR, reported to Sentry):
- `message to edit not found`
- `message is not modified`
- `MESSAGE_ID_INVALID`
- `text must be non-empty`

### Layer 2: `telegram_utils.rs` (safe_* wrapper functions)

Already handles some errors gracefully:
- `safe_edit_message_text` — catches `MessageNotModified`, `MessageToEditNotFound`, `MessageIdInvalid`, `NotEnoughRights*`, `MessageTextIsEmpty`
- `safe_edit_message_reply_markup` — catches same except `MessageTextIsEmpty`
- `safe_send_message` — catches `NotEnoughRights*`, `MessageTextIsEmpty`
- `safe_send_message_with_reply` — catches `MessageToReplyNotFound`, `NotEnoughRights*`, `MessageTextIsEmpty`

**Problem**: Many handlers bypass safe_* functions and call `bot.send_message(...).send().await?` or `bot.edit_message_text(...).send().await?` directly. These raw calls propagate Telegram API errors as `Err`, which bubbles up through the `log_handler` macro and into the `custom_error_handler`, creating duplicate Sentry events.

## Design

### Change 1: Update `classify_error()` in `custom_error_handler.rs`

Add missing Telegram message error patterns to the WARN classification:

```rust
// Telegram message errors — expected in production when messages are deleted
// between requests or users click the same button
if error_string.contains("message to edit not found")
    || error_string.contains("message is not modified")
    || error_string.contains("MESSAGE_ID_INVALID")
    || error_string.contains("text must be non-empty")
{
    return log::Level::Warn;
}
```

Also add an early return for `message is not modified` (same pattern as existing `message to be replied not found`):

```rust
if error_string.contains("message is not modified") {
    log::debug!("Ignoring Telegram not-modified error: {:?}", error);
    return;
}
```

### Change 2: Add new safe_* functions in `telegram_utils.rs`

**`safe_send_document`** — for document sending (download/mod.rs):
- `NotEnoughRightsToPostMessages` → Ok(()) — suppress, bot lacks permissions
- Network errors → Err (propagate)

**`safe_answer_callback_query`** — for callback query responses (settings/mod.rs):
- All errors → Ok(()), log at warn level — callback responses are non-critical

**`safe_delete_message`** — for message deletion (download/mod.rs):
- `MessageToDeleteNotFound` → Ok(())
- `NotEnoughRights*` → Ok(())

### Change 3: Replace raw Telegram API calls in handlers

Replace all direct `bot.send_message(...).send().await?` calls with `safe_send_message` / `safe_send_message_with_reply` in:

- `download/mod.rs` — lines 200, 204, 269, 345-357
- `book/mod.rs` — lines 63-66, 75, 79, 132, 145-148, 152-155, 161, 169, 173-176
- `search/mod.rs` — lines 65, 92, 110, 118, 177, 207, 239
- `annotations/mod.rs` — lines 97-99, 123-126
- `settings/mod.rs` — lines 52-55, 166-168, 310-312, 319-321
- `support/mod.rs` — lines 59-61

Replace `bot.edit_message_text(...)` with `safe_edit_message_text` in:
- `download/mod.rs` — lines 345-357 (send_archive_link)

Replace `bot.delete_message(...)` with `safe_delete_message` in:
- `download/mod.rs` — lines 105, 167, 460

Replace `bot.answer_callback_query(...)` with `safe_answer_callback_query` in:
- `settings/mod.rs` — lines 188, 200, 226, 241-245, 256-259

### Out of Scope

- `bot.copy_message` in `_send_cached` — different semantics (sending from channel)
- Retry logic for `Retry after` — teloxide `Throttle` adaptor already handles this
- Handler architecture changes — only wrapping calls
- `send_photo` in annotations — needs separate safe_ wrapper if it causes issues (currently not in Sentry)

## Expected Outcome

- Sentry ERROR events from book-bot drop by ~90% (the 4 issue types account for ~12000+ events)
- All 4 Sentry issues can be resolved and will not recur as ERROR
- Bot behavior remains identical — errors are still logged at WARN/DEBUG, just not sent to Sentry as errors
- No functional changes to user-facing behavior