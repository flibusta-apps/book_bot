# UX: Callback Feedback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the seven UX bugs identified in `docs/specs/10-ux-callback-feedback.md`: unanswered callback queries (eternal spinner), an unbounded `wait_archive` poll loop, random-module replies routed to DMs instead of the pressed button's chat, a swallowed API error in `update_history`, an asymmetric "±5 pages" pagination button, a false-error report on a successful annotation-photo send, and hardcoded Russian strings that duplicate existing constants.

**Architecture:** No new abstractions or dptree restructuring — each callback handler gets a `safe_answer_callback_query(&bot, cq.id.clone())` call as its first statement (cheapest way to guarantee the spinner clears in ≤1-2s regardless of downstream latency), the `wait_archive` poll loop gets a wall-clock bound checked each tick, `random` handlers get a `chat_id` computed once from `cq.message` with `cq.from.id` as fallback, and hardcoded strings are replaced with `utils/constants.rs` entries (two new constants added: `ERROR_RESTART`, `ANNOTATION_UNAVAILABLE`).

**Tech Stack:** Rust, teloxide 0.17 (via `CacheMe<Throttle<Bot>>` adaptor stack), tokio. Workspace root: `/Users/kurbezz/Projects/books_project/book_bot`. Crate root: `book_bot/` (package name `book_bot`, binary crate — run tests with `cargo test -p book_bot <filter>` from the workspace root).

## Global Constraints

- Never add a `Co-Authored-By: Claude` trailer to commits in this repository — end every commit message at its natural content.
- Do not touch any `callback_data.rs` wire format (`Display`/`FromStr` impls) — this spec is about UX responsiveness and copy, not parsing. No task in this plan edits a `callback_data.rs` file.
- Reuse existing `utils/constants.rs` entries wherever the literal text already matches one (`ERROR_TRY_LATER`, `ERROR_TRY_AGAIN`, `NOT_FOUND`, `BOOKS_NOT_FOUND`, etc.). Only add a new constant when the spec's literal has no existing match (`ERROR_RESTART` for "Ошибка! Начните заново :(", `ANNOTATION_UNAVAILABLE` for "Аннотация недоступна :(").
- Always call the existing `safe_answer_callback_query` / `safe_answer_callback_query_with_text` wrappers from `utils/telegram_utils.rs` — never call `bot.answer_callback_query(...)` directly.
- Every task must leave `cargo build` (workspace) green before it is committed; tasks that touch pure logic (pagination) must also leave `cargo test -p book_bot` green.

---

### Task 1: Fix the asymmetric "±5 pages" pagination button (spec 10.5)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/utils/pagination.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: no signature changes — `generic_get_pagination_keyboard<T>(page: u32, total_pages: u32, search_data: T, with_five: bool) -> InlineKeyboardMarkup` behavior only.

- [ ] **Step 1: Write the failing test**

Add to the bottom of `book_bot/src/bots/approved_bot/modules/utils/pagination.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct TestCallbackData;

    impl GetPaginationCallbackData for TestCallbackData {
        fn get_pagination_callback_data(&self, target_page: u32) -> String {
            format!("page_{target_page}")
        }
    }

    fn button_texts(keyboard: &InlineKeyboardMarkup) -> Vec<String> {
        keyboard
            .inline_keyboard
            .iter()
            .flatten()
            .map(|b| b.text.clone())
            .collect()
    }

    #[test]
    fn forward_five_jump_available_when_last_page_is_exactly_five_ahead() {
        // page 1, 6 total pages: page 1 + 5 = page 6, which exists.
        let keyboard = generic_get_pagination_keyboard(1, 6, TestCallbackData, true);
        let texts = button_texts(&keyboard);
        assert!(
            texts.iter().any(|t| t == "> 5 >"),
            "expected a '> 5 >' button, got {texts:?}"
        );
    }

    #[test]
    fn forward_five_jump_unavailable_when_out_of_range() {
        // page 1, 5 total pages: page 1 + 5 = page 6, which does not exist.
        let keyboard = generic_get_pagination_keyboard(1, 5, TestCallbackData, true);
        let texts = button_texts(&keyboard);
        assert!(
            !texts.iter().any(|t| t == "> 5 >"),
            "did not expect a '> 5 >' button, got {texts:?}"
        );
    }

    #[test]
    fn backward_five_jump_available_when_reachable() {
        let keyboard = generic_get_pagination_keyboard(6, 10, TestCallbackData, true);
        let texts = button_texts(&keyboard);
        assert!(
            texts.iter().any(|t| t == "< 5 <"),
            "expected a '< 5 <' button, got {texts:?}"
        );
    }

    #[test]
    fn backward_five_jump_unavailable_when_out_of_range() {
        let keyboard = generic_get_pagination_keyboard(5, 10, TestCallbackData, true);
        let texts = button_texts(&keyboard);
        assert!(
            !texts.iter().any(|t| t == "< 5 <"),
            "did not expect a '< 5 <' button, got {texts:?}"
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify `forward_five_jump_available_when_last_page_is_exactly_five_ahead` fails**

Run (from the workspace root `/Users/kurbezz/Projects/books_project/book_bot`):

```bash
cargo test -p book_bot pagination::tests
```

Expected: `forward_five_jump_available_when_last_page_is_exactly_five_ahead` FAILS (assertion panic, empty button list); the other three pass already.

- [ ] **Step 3: Fix the off-by-one**

In `generic_get_pagination_keyboard`, change:

```rust
            if t_page + 5 < total_pages.into() {
```

to:

```rust
            if t_page + 5 <= total_pages.into() {
```

- [ ] **Step 4: Run the tests to verify all four pass**

Run: `cargo test -p book_bot pagination::tests`
Expected: 4 passed, 0 failed.

- [ ] **Step 5: Commit**

```bash
cd book_bot && git add src/bots/approved_bot/modules/utils/pagination.rs && git commit -m "fix: allow forward 5-page pagination jump to the exact last page

t_page + 5 < total_pages was strict, so pressing '> 5 >' from page 1 with
exactly 6 total pages had no effect even though page 6 exists. Use <=."
```

---

### Task 2: Fix annotations module — answer callback, stop reporting a successful photo send as an error (spec 10.1, 10.6, 10.7)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/utils/constants.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/annotations/mod.rs`
- Delete: `book_bot/src/bots/approved_bot/modules/annotations/errors.rs`

**Interfaces:**
- Consumes: `safe_answer_callback_query(bot: &CacheMe<Throttle<Bot>>, callback_query_id: CallbackQueryId) -> BotHandlerInternal` (already defined in `utils/telegram_utils.rs`).
- Produces: `pub const ANNOTATION_UNAVAILABLE: &str = "Аннотация недоступна :("` in `utils/constants.rs`.

- [ ] **Step 1: Add the missing constant**

In `book_bot/src/bots/approved_bot/modules/utils/constants.rs`, add after `pub const RATE_LIMIT_ERROR: &str = "Слишком много запросов, попробуйте позже";`:

```rust
pub const ANNOTATION_UNAVAILABLE: &str = "Аннотация недоступна :(";
```

- [ ] **Step 2: Update imports in `annotations/mod.rs`**

Remove the `pub mod errors;` declaration:

```rust
pub mod callback_data;
pub mod commands;
pub mod errors;
pub mod formatter;
```

becomes:

```rust
pub mod callback_data;
pub mod commands;
pub mod formatter;
```

Change:

```rust
use crate::bots::{
    approved_bot::{
        modules::utils::{
            message_text::is_message_text_equals,
            pagination::generic_get_pagination_keyboard,
            telegram_utils::{
                safe_edit_message_text, safe_send_message_with_reply, safe_send_photo,
            },
        },
        services::book_library::{get_author_annotation, get_book_annotation},
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};
```

to:

```rust
use crate::bots::{
    approved_bot::{
        modules::utils::{
            constants::ANNOTATION_UNAVAILABLE,
            message_text::is_message_text_equals,
            pagination::generic_get_pagination_keyboard,
            telegram_utils::{
                safe_answer_callback_query, safe_edit_message_text, safe_send_message_with_reply,
                safe_send_photo,
            },
        },
        services::book_library::{get_author_annotation, get_book_annotation},
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};
```

Change:

```rust
use self::{
    callback_data::AnnotationCallbackData, commands::AnnotationCommand,
    errors::AnnotationFormatError, formatter::AnnotationFormat,
};
```

to:

```rust
use self::{
    callback_data::AnnotationCallbackData, commands::AnnotationCommand, formatter::AnnotationFormat,
};
```

- [ ] **Step 3: Replace the two hardcoded "Аннотация недоступна :(" literals**

In `send_annotation_handler`, the first occurrence:

```rust
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
```

becomes:

```rust
    let annotation = match annotation_getter(id).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            return safe_send_message_with_reply(
                &bot,
                message.chat.id,
                ANNOTATION_UNAVAILABLE,
                ReplyParameters::new(message.id),
                None,
            )
            .await;
        }
        Err(err) => return Err(err),
    };
```

The second occurrence:

```rust
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
```

becomes:

```rust
    if annotation.get_file().is_none() && !annotation.is_normal_text() {
        return safe_send_message_with_reply(
            &bot,
            message.chat.id,
            ANNOTATION_UNAVAILABLE,
            ReplyParameters::new(message.id),
            None,
        )
        .await;
    };
```

- [ ] **Step 4: Stop reporting a successful photo send as an error (spec 10.6)**

By this point in `send_annotation_handler`, `annotation.get_file().is_none() && !annotation.is_normal_text()` has already returned above, so reaching this check means `annotation.get_file()` is `Some` — the photo branch above already ran (and, if it succeeded, the photo is already in the chat). Change:

```rust
    if !annotation.is_normal_text() {
        return Err(AnnotationFormatError {
            _command: command,
            _text: annotation.get_text().to_string(),
        }
        .into());
    }
```

to:

```rust
    if !annotation.is_normal_text() {
        return Ok(());
    }
```

- [ ] **Step 5: Delete the now-unused error type**

Delete the file `book_bot/src/bots/approved_bot/modules/annotations/errors.rs`.

- [ ] **Step 6: Answer the callback query in the pagination handler (spec 10.1)**

In `annotation_pagination_handler`, add as the first statement of the function body (right after the opening `{`, before `let (id, page) = match callback_data { ... }`):

```rust
    safe_answer_callback_query(&bot, cq.id.clone()).await?;
```

- [ ] **Step 7: Verify it builds and existing tests still pass**

Run: `cargo build -p book_bot && cargo test -p book_bot annotations`
Expected: no compile errors (confirms `AnnotationFormatError` has no remaining references), existing `annotations::commands` tests still pass.

- [ ] **Step 8: Commit**

```bash
cd book_bot && git add src/bots/approved_bot/modules/utils/constants.rs src/bots/approved_bot/modules/annotations/mod.rs src/bots/approved_bot/modules/annotations/errors.rs && git commit -m "fix: answer annotation callback queries, stop erroring on a successful photo send

- annotation_pagination_handler never called answer_callback_query, so the
  Telegram client showed a loading spinner until timeout on every page press.
- send_annotation_handler returned Err(AnnotationFormatError) after already
  successfully sending the annotation photo, landing a success in error logs.
  AnnotationFormatError's fields were never read (dead_code), so delete it.
- Replace two hardcoded 'Аннотация недоступна :(' literals with the new
  ANNOTATION_UNAVAILABLE constant."
```

---

### Task 3: Replace hardcoded error literals in settings module (spec 10.7)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/settings/mod.rs`

**Interfaces:**
- Consumes: existing `pub const ERROR_TRY_AGAIN: &str = "Ошибка! Попробуйте заново(";` from `utils/constants.rs`.
- Produces: nothing new (pure literal-to-constant substitution, no behavior change).

- [ ] **Step 1: Import the constant**

Change:

```rust
use crate::bots::{
    approved_bot::{
        modules::utils::telegram_utils::{
            safe_answer_callback_query, safe_answer_callback_query_with_text,
            safe_edit_message_reply_markup, safe_edit_message_text, safe_send_message,
        },
        services::user_settings::{
            create_or_update_user_settings, get_langs, get_user_or_default_lang_codes,
            get_user_settings, DefaultSearchType, FileNameLang, Lang,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};
```

to:

```rust
use crate::bots::{
    approved_bot::{
        modules::utils::{
            constants::ERROR_TRY_AGAIN,
            telegram_utils::{
                safe_answer_callback_query, safe_answer_callback_query_with_text,
                safe_edit_message_reply_markup, safe_edit_message_text, safe_send_message,
            },
        },
        services::user_settings::{
            create_or_update_user_settings, get_langs, get_user_or_default_lang_codes,
            get_user_settings, DefaultSearchType, FileNameLang, Lang,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};
```

- [ ] **Step 2: Replace the literal in `settings_callback_handler`'s message-not-found branch**

Change:

```rust
    let message = match cq.message {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Попробуйте заново(", None).await?;
            return Ok(());
        }
    };
```

to:

```rust
    let message = match cq.message {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), ERROR_TRY_AGAIN, None).await?;
            return Ok(());
        }
    };
```

- [ ] **Step 3: Replace both identical literals in the `safe_answer_callback_query_with_text` calls**

This exact block appears twice (once under `SettingsCallbackData::DefaultSearch`, once under `SettingsCallbackData::FileNameLang`) — replace both occurrences:

```rust
                safe_answer_callback_query_with_text(
                    &bot,
                    cq.id,
                    "Ошибка! Попробуйте заново(",
                    true,
                )
                .await?;
```

to:

```rust
                safe_answer_callback_query_with_text(&bot, cq.id, ERROR_TRY_AGAIN, true).await?;
```

- [ ] **Step 4: Replace the literal after `create_or_update_user_settings` fails**

Change:

```rust
        safe_send_message(&bot, message.chat().id, "Ошибка! Попробуйте заново(", None).await?;
        return Err(err);
    }

    let all_langs = match get_langs().await {
```

to:

```rust
        safe_send_message(&bot, message.chat().id, ERROR_TRY_AGAIN, None).await?;
        return Err(err);
    }

    let all_langs = match get_langs().await {
```

- [ ] **Step 5: Replace the literal after `get_langs` fails**

Change:

```rust
        Err(err) => {
            safe_send_message(&bot, message.chat().id, "Ошибка! Попробуйте заново(", None).await?;
            return Err(err);
        }
    };
```

to:

```rust
        Err(err) => {
            safe_send_message(&bot, message.chat().id, ERROR_TRY_AGAIN, None).await?;
            return Err(err);
        }
    };
```

- [ ] **Step 6: Verify no literal remains and the crate builds**

Run:

```bash
grep -n '"Ошибка! Попробуйте заново("' book_bot/src/bots/approved_bot/modules/settings/mod.rs
cargo build -p book_bot
```

Expected: grep prints nothing; build succeeds.

- [ ] **Step 7: Commit**

```bash
cd book_bot && git add src/bots/approved_bot/modules/settings/mod.rs && git commit -m "refactor: use ERROR_TRY_AGAIN constant instead of duplicated literal in settings module

Pure substitution, no behavior change — the literal already matched an
existing constants.rs entry that just wasn't being used here."
```

---

### Task 4: Answer callback query in book module's pagination handler (spec 10.1)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/book/mod.rs`

**Interfaces:**
- Consumes: `safe_answer_callback_query`.
- Produces: nothing new.

- [ ] **Step 1: Import `safe_answer_callback_query`**

Change:

```rust
use super::utils::{
    filter_command::filter_command,
    pagination::generic_get_pagination_keyboard,
    telegram_utils::{safe_edit_message_text, safe_send_message, safe_send_message_with_reply},
};
```

to:

```rust
use super::utils::{
    filter_command::filter_command,
    pagination::generic_get_pagination_keyboard,
    telegram_utils::{
        safe_answer_callback_query, safe_edit_message_text, safe_send_message,
        safe_send_message_with_reply,
    },
};
```

- [ ] **Step 2: Answer the callback as the first statement of `send_pagination_book_handler`**

Change:

```rust
    let (id, page) = match callback_data {
        BookCallbackData::Author { id, page } => (id, page),
        BookCallbackData::Translator { id, page } => (id, page),
        BookCallbackData::Sequence { id, page } => (id, page),
    };

    let chat_id = cq.message.as_ref().map(|message| message.chat().id);
```

to:

```rust
    safe_answer_callback_query(&bot, cq.id.clone()).await?;

    let (id, page) = match callback_data {
        BookCallbackData::Author { id, page } => (id, page),
        BookCallbackData::Translator { id, page } => (id, page),
        BookCallbackData::Sequence { id, page } => (id, page),
    };

    let chat_id = cq.message.as_ref().map(|message| message.chat().id);
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p book_bot`
Expected: no compile errors.

- [ ] **Step 4: Commit**

```bash
cd book_bot && git add src/bots/approved_bot/modules/book/mod.rs && git commit -m "fix: answer callback query in book pagination handler

send_pagination_book_handler never called answer_callback_query, so the
Telegram client showed a loading spinner until timeout on every page press,
including the 'nothing changed' early-return paths."
```

---

### Task 5: Answer callback query in search module's pagination handler (spec 10.1)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/search/mod.rs`

**Interfaces:**
- Consumes: `safe_answer_callback_query`.
- Produces: nothing new.

- [ ] **Step 1: Import `safe_answer_callback_query`**

Change:

```rust
use crate::bots::{
    approved_bot::{
        modules::utils::{
            message_text::is_message_text_equals,
            telegram_utils::{
                safe_edit_message_text, safe_send_message, safe_send_message_with_reply,
            },
        },
```

to:

```rust
use crate::bots::{
    approved_bot::{
        modules::utils::{
            message_text::is_message_text_equals,
            telegram_utils::{
                safe_answer_callback_query, safe_edit_message_text, safe_send_message,
                safe_send_message_with_reply,
            },
        },
```

- [ ] **Step 2: Answer the callback as the first statement of `generic_search_pagination_handler`**

Change:

```rust
    let chat_id = cq.chat_id();
    let user_id = cq.from.id;
```

to:

```rust
    safe_answer_callback_query(&bot, cq.id.clone()).await?;

    let chat_id = cq.chat_id();
    let user_id = cq.from.id;
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p book_bot`
Expected: no compile errors.

- [ ] **Step 4: Commit**

```bash
cd book_bot && git add src/bots/approved_bot/modules/search/mod.rs && git commit -m "fix: answer callback query in search pagination handler

generic_search_pagination_handler never called answer_callback_query, so the
Telegram client showed a loading spinner until timeout on every page press,
including the is_message_text_equals early-return path."
```

---

### Task 6: Answer callback query and stop swallowing API errors in update_history (spec 10.1, 10.4)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/update_history/mod.rs`

**Interfaces:**
- Consumes: `safe_answer_callback_query`, existing `ERROR_TRY_LATER` constant.
- Produces: nothing new.

- [ ] **Step 1: Import `safe_answer_callback_query` and `ERROR_TRY_LATER`**

Change:

```rust
use crate::bots::{
    approved_bot::{
        modules::utils::{
            constants::{ERROR_TRY_AGAIN, TELEGRAM_MESSAGE_MAX_LENGTH},
            message_text::is_message_text_equals,
            telegram_utils::{safe_edit_message_text, safe_send_message},
        },
```

to:

```rust
use crate::bots::{
    approved_bot::{
        modules::utils::{
            constants::{ERROR_TRY_AGAIN, ERROR_TRY_LATER, TELEGRAM_MESSAGE_MAX_LENGTH},
            message_text::is_message_text_equals,
            telegram_utils::{safe_answer_callback_query, safe_edit_message_text, safe_send_message},
        },
```

- [ ] **Step 2: Answer the callback as the first statement of `update_log_pagination_handler`**

Change:

```rust
    let message = match cq.message.clone() {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), ERROR_TRY_AGAIN, None).await?;
            return Ok(());
        }
    };
```

to:

```rust
    safe_answer_callback_query(&bot, cq.id.clone()).await?;

    let message = match cq.message.clone() {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), ERROR_TRY_AGAIN, None).await?;
            return Ok(());
        }
    };
```

- [ ] **Step 3: Send `ERROR_TRY_LATER` before propagating the first `get_uploaded_books` error (spec 10.4)**

Change:

```rust
    let mut items_page = match get_uploaded_books(
        update_callback_data.page,
        update_callback_data
            .from
            .format("%Y-%m-%d")
            .to_string()
            .into(),
        update_callback_data
            .to
            .format("%Y-%m-%d")
            .to_string()
            .into(),
    )
    .await?
    {
        Some(v) => v,
        None => {
            safe_send_message(
                &bot,
                message.chat().id,
                "Нет новых книг за этот период.",
                None,
            )
            .await?;
            return Ok(());
        }
    };
```

to:

```rust
    let mut items_page = match get_uploaded_books(
        update_callback_data.page,
        update_callback_data
            .from
            .format("%Y-%m-%d")
            .to_string()
            .into(),
        update_callback_data
            .to
            .format("%Y-%m-%d")
            .to_string()
            .into(),
    )
    .await
    {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(
                &bot,
                message.chat().id,
                "Нет новых книг за этот период.",
                None,
            )
            .await?;
            return Ok(());
        }
        Err(err) => {
            safe_send_message(&bot, message.chat().id, ERROR_TRY_LATER, None).await?;
            return Err(err);
        }
    };
```

- [ ] **Step 4: Send `ERROR_TRY_LATER` before propagating the second `get_uploaded_books` error**

Change:

```rust
    if update_callback_data.page > items_page.pages {
        items_page = match get_uploaded_books(
            items_page.pages,
            update_callback_data
                .from
                .format("%Y-%m-%d")
                .to_string()
                .into(),
            update_callback_data
                .to
                .format("%Y-%m-%d")
                .to_string()
                .into(),
        )
        .await?
        {
            Some(v) => v,
            None => {
                safe_send_message(
                    &bot,
                    message.chat().id,
                    "Нет новых книг за этот период.",
                    None,
                )
                .await?;
                return Ok(());
            }
        };
    }
```

to:

```rust
    if update_callback_data.page > items_page.pages {
        items_page = match get_uploaded_books(
            items_page.pages,
            update_callback_data
                .from
                .format("%Y-%m-%d")
                .to_string()
                .into(),
            update_callback_data
                .to
                .format("%Y-%m-%d")
                .to_string()
                .into(),
        )
        .await
        {
            Ok(Some(v)) => v,
            Ok(None) => {
                safe_send_message(
                    &bot,
                    message.chat().id,
                    "Нет новых книг за этот период.",
                    None,
                )
                .await?;
                return Ok(());
            }
            Err(err) => {
                safe_send_message(&bot, message.chat().id, ERROR_TRY_LATER, None).await?;
                return Err(err);
            }
        };
    }
```

- [ ] **Step 5: Verify it builds**

Run: `cargo build -p book_bot`
Expected: no compile errors.

- [ ] **Step 6: Commit**

```bash
cd book_bot && git add src/bots/approved_bot/modules/update_history/mod.rs && git commit -m "fix: answer callback query and send an error message before propagating get_uploaded_books failures

update_log_pagination_handler never called answer_callback_query. It also
propagated get_uploaded_books errors via '?' with no message to the user,
unlike book/search/download which send ERROR_TRY_LATER first."
```

---

### Task 7: Fix random module — answer callback, route replies to the button's chat, use constants (spec 10.1, 10.3, 10.7)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/utils/constants.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/random/mod.rs`

**Interfaces:**
- Consumes: `safe_answer_callback_query`, existing `NOT_FOUND` / `ERROR_TRY_LATER` constants.
- Produces: `pub const ERROR_RESTART: &str = "Ошибка! Начните заново :(";` in `utils/constants.rs`.

- [ ] **Step 1: Add the missing constant**

In `book_bot/src/bots/approved_bot/modules/utils/constants.rs`, add after `pub const ERROR_TRY_AGAIN: &str = "Ошибка! Попробуйте заново(";`:

```rust
pub const ERROR_RESTART: &str = "Ошибка! Начните заново :(";
```

- [ ] **Step 2: Update imports in `random/mod.rs`**

Change:

```rust
use crate::bots::{
    approved_bot::{
        modules::random::callback_data::RandomCallbackData,
        modules::utils::telegram_utils::{
            safe_edit_message_reply_markup, safe_send_message, safe_send_message_with_reply,
        },
        services::{
            book_library::{self, formatters::Format},
            user_settings::get_user_or_default_lang_codes,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};
```

to:

```rust
use crate::bots::{
    approved_bot::{
        modules::random::callback_data::RandomCallbackData,
        modules::utils::{
            constants::{ERROR_RESTART, ERROR_TRY_LATER, NOT_FOUND},
            telegram_utils::{
                safe_answer_callback_query, safe_edit_message_reply_markup, safe_send_message,
                safe_send_message_with_reply,
            },
        },
        services::{
            book_library::{self, formatters::Format},
            user_settings::get_user_or_default_lang_codes,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};
```

- [ ] **Step 3: Fix `get_random_item_handler_internal`**

Change the whole function:

```rust
async fn get_random_item_handler_internal<T>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    item: anyhow::Result<Option<T>>,
) -> BotHandlerInternal
where
    T: Format,
{
    let item = match item {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, cq.from.id.into(), "Не найдено :(", None).await?;
            return Ok(());
        }
        Err(err) => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Попробуйте позже :(", None).await?;
            return Err(err);
        }
    };

    let item_message = item.format(4096).result;

    safe_send_message(
        &bot,
        cq.from.id.into(),
        item_message,
        Some(InlineKeyboardMarkup {
            inline_keyboard: vec![vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    cq.data.clone().unwrap_or_default(),
                ),
                text: String::from("Повторить?"),
            }]],
        }),
    )
    .await?;

    match cq.message {
        Some(message) => {
            safe_edit_message_reply_markup(
                &bot,
                message.chat().id,
                message.id(),
                InlineKeyboardMarkup {
                    inline_keyboard: vec![],
                },
            )
            .await?;
            Ok(())
        }
        None => Ok(()),
    }
}
```

to:

```rust
async fn get_random_item_handler_internal<T>(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    item: anyhow::Result<Option<T>>,
) -> BotHandlerInternal
where
    T: Format,
{
    safe_answer_callback_query(&bot, cq.id.clone()).await?;

    let chat_id = cq
        .message
        .as_ref()
        .map(|m| m.chat().id)
        .unwrap_or_else(|| cq.from.id.into());

    let item = match item {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, chat_id, NOT_FOUND, None).await?;
            return Ok(());
        }
        Err(err) => {
            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
            return Err(err);
        }
    };

    let item_message = item.format(4096).result;

    safe_send_message(
        &bot,
        chat_id,
        item_message,
        Some(InlineKeyboardMarkup {
            inline_keyboard: vec![vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    cq.data.clone().unwrap_or_default(),
                ),
                text: String::from("Повторить?"),
            }]],
        }),
    )
    .await?;

    match cq.message {
        Some(message) => {
            safe_edit_message_reply_markup(
                &bot,
                message.chat().id,
                message.id(),
                InlineKeyboardMarkup {
                    inline_keyboard: vec![],
                },
            )
            .await?;
            Ok(())
        }
        None => Ok(()),
    }
}
```

- [ ] **Step 4: Fix `get_genre_metas_handler`**

Change the whole function:

```rust
#[log_handler("random")]
async fn get_genre_metas_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let genre_metas = match book_library::get_genre_metas().await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, cq.from.id.into(), "Не найдено :(", None).await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let message = match cq.message {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Начните заново :(", None).await?;
            return Ok(());
        }
    };
```

(keep the rest of the function — keyboard building and `safe_edit_message_reply_markup(&bot, message.chat().id, message.id(), keyboard).await?;` — unchanged) to:

```rust
#[log_handler("random")]
async fn get_genre_metas_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    safe_answer_callback_query(&bot, cq.id.clone()).await?;

    let chat_id = cq
        .message
        .as_ref()
        .map(|m| m.chat().id)
        .unwrap_or_else(|| cq.from.id.into());

    let genre_metas = match book_library::get_genre_metas().await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, chat_id, NOT_FOUND, None).await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let message = match cq.message {
        Some(v) => v,
        None => {
            safe_send_message(&bot, chat_id, ERROR_RESTART, None).await?;
            return Ok(());
        }
    };
```

- [ ] **Step 5: Fix `get_genres_by_meta_handler`**

Change the whole function:

```rust
#[log_handler("random")]
async fn get_genres_by_meta_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    genre_index: u32,
) -> BotHandlerInternal {
    let genre_metas = match book_library::get_genre_metas().await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, cq.from.id.into(), "Не найдено :(", None).await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let meta = match genre_metas.get(genre_index as usize) {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Попробуйте позже :(", None).await?;

            return Ok(());
        }
    };

    let genres_page = match book_library::get_genres(meta.into()).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, cq.from.id.into(), "Не найдено :(", None).await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };
```

(keep the button-building code unchanged) then change:

```rust
    let message = match cq.message {
        Some(message) => message,
        None => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Начните заново :(", None).await?;

            return Ok(());
        }
    };
```

Overall new function:

```rust
#[log_handler("random")]
async fn get_genres_by_meta_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    genre_index: u32,
) -> BotHandlerInternal {
    safe_answer_callback_query(&bot, cq.id.clone()).await?;

    let chat_id = cq
        .message
        .as_ref()
        .map(|m| m.chat().id)
        .unwrap_or_else(|| cq.from.id.into());

    let genre_metas = match book_library::get_genre_metas().await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, chat_id, NOT_FOUND, None).await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let meta = match genre_metas.get(genre_index as usize) {
        Some(v) => v,
        None => {
            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;

            return Ok(());
        }
    };

    let genres_page = match book_library::get_genres(meta.into()).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(&bot, chat_id, NOT_FOUND, None).await?;
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let mut buttons: Vec<Vec<InlineKeyboardButton>> = genres_page
        .items
        .into_iter()
        .map(|genre| {
            vec![InlineKeyboardButton {
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    RandomCallbackData::RandomBookByGenre { id: genre.id }.to_string(),
                ),
                text: genre.description,
            }]
        })
        .collect();

    buttons.push(vec![InlineKeyboardButton {
        kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
            RandomCallbackData::RandomBookByGenreRequest.to_string(),
        ),
        text: "< Назад >".to_string(),
    }]);

    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: buttons,
    };

    let message = match cq.message {
        Some(message) => message,
        None => {
            safe_send_message(&bot, chat_id, ERROR_RESTART, None).await?;

            return Ok(());
        }
    };

    safe_edit_message_reply_markup(&bot, message.chat().id, message.id(), keyboard).await?;

    Ok(())
}
```

- [ ] **Step 6: Verify it builds**

Run: `cargo build -p book_bot`
Expected: no compile errors.

- [ ] **Step 7: Commit**

```bash
cd book_bot && git add src/bots/approved_bot/modules/utils/constants.rs src/bots/approved_bot/modules/random/mod.rs && git commit -m "fix: answer callback queries and route random-module replies to the button's chat

All three random handlers built a ChatId from cq.from.id (the user's DM),
so in a group chat results and errors went to the presser's DM instead of
the group — and silently failed if the user never started the bot. Compute
chat_id from cq.message (the group/chat the button lives in) with cq.from.id
only as a fallback when no message is attached. Also answer every callback
query so the loading spinner clears, and replace hardcoded literals with
constants (adding the missing ERROR_RESTART)."
```

---

### Task 8: Fix download module — answer callback, bound the archive-wait poll loop (spec 10.1, 10.2)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/download/mod.rs`

**Interfaces:**
- Consumes: `safe_answer_callback_query`, existing `get_check_keyboard(task_id: String) -> InlineKeyboardMarkup`.
- Produces: no signature changes — `wait_archive` now returns `Ok(())` with a "check later" message instead of polling forever.

- [ ] **Step 1: Import `safe_answer_callback_query` and `Instant`**

Change:

```rust
use super::utils::constants::*;
use super::utils::telegram_utils::{
    safe_copy_message, safe_delete_message, safe_edit_message_text, safe_edit_message_text_html,
    safe_send_document, safe_send_message_with_reply,
};
```

to:

```rust
use super::utils::constants::*;
use super::utils::telegram_utils::{
    safe_answer_callback_query, safe_copy_message, safe_delete_message, safe_edit_message_text,
    safe_edit_message_text_html, safe_send_document, safe_send_message_with_reply,
};
```

Change:

```rust
use std::time::Duration;
```

to:

```rust
use std::time::{Duration, Instant};
```

- [ ] **Step 2: Bound the `wait_archive` poll loop (spec 10.2)**

Change:

```rust
async fn wait_archive(
    bot: CacheMe<Throttle<Bot>>,
    task_id: String,
    input_message: MaybeInaccessibleMessage,
) -> BotHandlerInternal {
    let mut interval = time::interval(Duration::from_secs(15));

    let message = match input_message {
        MaybeInaccessibleMessage::Regular(message) => message,
        _ => {
            send_error_message(&bot, input_message.chat().id, input_message.id()).await;
            return Ok(());
        }
    };

    let task = loop {
        interval.tick().await;

        let task = match get_task(&task_id).await {
```

to:

```rust
async fn wait_archive(
    bot: CacheMe<Throttle<Bot>>,
    task_id: String,
    input_message: MaybeInaccessibleMessage,
) -> BotHandlerInternal {
    const MAX_WAIT: Duration = Duration::from_secs(45 * 60);

    let mut interval = time::interval(Duration::from_secs(15));

    let message = match input_message {
        MaybeInaccessibleMessage::Regular(message) => message,
        _ => {
            send_error_message(&bot, input_message.chat().id, input_message.id()).await;
            return Ok(());
        }
    };

    let start = Instant::now();

    let task = loop {
        interval.tick().await;

        if start.elapsed() > MAX_WAIT {
            log::warn!("wait_archive timed out for task {task_id} after {MAX_WAIT:?}");
            safe_edit_message_text(
                &bot,
                message.chat.id,
                message.id,
                "Архив готовится дольше обычного. Проверьте статус позже.",
                Some(get_check_keyboard(task_id.clone())),
            )
            .await?;
            return Ok(());
        }

        let task = match get_task(&task_id).await {
```

(the rest of the loop body — the `Err`/status-check/`safe_edit_message_text` progress update — stays exactly as-is; only the two new statements above are inserted before the existing `let task = match get_task(&task_id).await { ... };` line).

- [ ] **Step 3: Answer the callback query at the top of `download_archive` (spec 10.1)**

Change:

```rust
#[log_handler("download")]
async fn download_archive(
    cq: CallbackQuery,
    download_archive_query_data: DownloadArchiveQueryData,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(cq.from.id).await;
```

to:

```rust
#[log_handler("download")]
async fn download_archive(
    cq: CallbackQuery,
    download_archive_query_data: DownloadArchiveQueryData,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    safe_answer_callback_query(&bot, cq.id.clone()).await?;

    let allowed_langs = get_user_or_default_lang_codes(cq.from.id).await;
```

- [ ] **Step 4: Answer the callback query at the top of `download_query_handler`**

Change:

```rust
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
```

to:

```rust
#[log_handler("download")]
async fn download_query_handler(
    cq: CallbackQuery,
    download_query_data: DownloadQueryData,
    bot: CacheMe<Throttle<Bot>>,
    cache: BotCache,
) -> BotHandlerInternal {
    safe_answer_callback_query(&bot, cq.id.clone()).await?;

    let Some(message) = cq.message else {
        return Ok(());
    };
    let user_id = Some(cq.from.id.0);
    download_handler(message, bot, cache, download_query_data, true, user_id).await
}
```

- [ ] **Step 5: Answer the callback query in the `CheckArchiveStatus` endpoint closure**

Change:

```rust
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
```

to:

```rust
        .branch(
            Update::filter_callback_query()
            .chain(filter_callback_query::<CheckArchiveStatus>())
            .endpoint(|cq: CallbackQuery, status: CheckArchiveStatus, bot: CacheMe<Throttle<Bot>>| async move {
                safe_answer_callback_query(&bot, cq.id.clone()).await?;

                let Some(message) = cq.message else {
                    return Ok(());
                };
                wait_archive(bot, status.task_id, message).await
            })
        )
```

- [ ] **Step 6: Verify it builds**

Run: `cargo build -p book_bot`
Expected: no compile errors.

- [ ] **Step 7: Commit**

```bash
cd book_bot && git add src/bots/approved_bot/modules/download/mod.rs && git commit -m "fix: answer download callback queries and bound the archive-wait poll loop

download_archive, download_query_handler, and the CheckArchiveStatus endpoint
never called answer_callback_query, leaving the loading spinner up until
Telegram's timeout. wait_archive also polled with no time bound — a task
stuck InProgress on the batch_downloader side left an eternal 15s-interval
loop per 'refresh status' press. Cap the wait at 45 minutes, then send a
'check later' message with the existing CheckArchiveStatus button and log
the timeout."
```

---

### Task 9: Final verification sweep

**Files:** none (verification only).

- [ ] **Step 1: Full workspace build**

Run (from the workspace root `/Users/kurbezz/Projects/books_project/book_bot`):

```bash
cargo build
```

Expected: builds cleanly, no warnings about unused imports (would indicate a leftover `use` from a deleted literal/type).

- [ ] **Step 2: Full test suite**

Run:

```bash
cargo test -p book_bot
```

Expected: all tests pass, including the 4 new `pagination::tests` from Task 1.

- [ ] **Step 3: Confirm no relocated literal remains outside `constants.rs`**

Run each of the following from `book_bot/src`; every command must print nothing:

```bash
grep -rn '"Ошибка! Попробуйте заново("' book_bot/src --include=*.rs | grep -v utils/constants.rs
grep -rn '"Не найдено :("' book_bot/src --include=*.rs | grep -v utils/constants.rs
grep -rn '"Ошибка! Начните заново :("' book_bot/src --include=*.rs | grep -v utils/constants.rs
grep -rn '"Аннотация недоступна :("' book_bot/src --include=*.rs | grep -v utils/constants.rs
```

(run from the workspace root, so paths resolve — adjust the leading path if your shell's cwd is already inside `book_bot/`)

- [ ] **Step 4: Confirm `AnnotationFormatError` has no remaining references**

Run:

```bash
grep -rn "AnnotationFormatError" book_bot/src
```

Expected: no output (type and file both deleted in Task 2).

- [ ] **Step 5: Confirm every callback module now answers its queries**

Run:

```bash
grep -rln "safe_answer_callback_query" book_bot/src/bots/approved_bot/modules/*/mod.rs
```

Expected: lists `annotations/mod.rs`, `book/mod.rs`, `search/mod.rs`, `update_history/mod.rs`, `random/mod.rs`, `download/mod.rs`, `settings/mod.rs` (settings already had it before this plan).

- [ ] **Step 6: No commit for this task** — it is verification-only. If any check above fails, go back to the relevant task, fix it there, and re-commit that task (do not create a separate "fix verification" commit).
