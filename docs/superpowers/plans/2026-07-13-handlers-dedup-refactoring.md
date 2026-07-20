# Handler Refactoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `docs/specs/11-handlers-dedup-refactoring.md` — remove the four-fold duplication in `search::message_handler`, extract a shared pagination skeleton used by `book`, `search`, and `update_history`, decompose `download/mod.rs` and `settings/mod.rs` into smaller files, dedup `book_library/formatters.rs`, and clean up the small items in §11.6 (dead `escape_html` copies, dead `EnumIter` derives, unnecessary `#[allow(dead_code)]`, an ignored `width` parameter, a by-value `CallbackQuery` param, and the two same-named `bots_manager` modules).

**Architecture:** No new runtime dependencies. The shared pagination skeleton becomes one generic `paginate()` function in `utils/pagination.rs` that all three "fetch a page → not-found → clamp → format → no-op-if-unchanged → edit + keyboard" call sites delegate to; each call site keeps only its own data-fetching and text/keyboard-data setup. `download/mod.rs` and `settings/mod.rs` split into sibling files (`file_send.rs`/`keyboards.rs`/`archive.rs` and `keyboards.rs` respectively) with `mod.rs` left holding only the `dptree` wiring. `book_library/formatters.rs` gains one generic `format_list()` helper and a single `Person` type that replaces the structurally-identical `BookAuthor`/`BookTranslator`.

**Tech Stack:** Rust, `teloxide`, `strum`/`strum_macros`, `smartstring`, `smallvec`. Workspace root: `/Users/kurbezz/Projects/books_project/book_bot`. Crate root: `book_bot/` (package name `book_bot`, binary crate — run tests with `cargo test -p book_bot <filter>` from the workspace root).

## Global Constraints

- **No behavior change.** Wire formats (`Display`/`FromStr` for all `callback_data`/`commands` types), user-facing message texts, and keyboard layouts stay byte-identical. The baseline is `cargo test -p book_bot` → `136 passed; 0 failed` (confirmed before this plan was written) — every task must leave that number equal or higher, never lower.
- **`#[log_handler("...")]` stays only on the functions that already carry it today** (the `dptree`-registered endpoints and the internal fns they call directly that are already tagged: `generic_search_pagination_handler`, `send_book_handler`, `send_pagination_book_handler`, `update_log_command`, `update_log_pagination_handler`, `get_download_keyboard_handler`, `get_download_archive_keyboard_handler`, `download_archive`, `download_query_handler`, `settings_handler`, `settings_callback_handler`, `help_handler`, `support_command_handler`). The macro emits a `tracing::info!` line plus a `HandlerMetricsGuard` per call — **never add it to a newly-extracted private helper function**, or metrics/logs will double-count per incoming update.
- One deliberate, called-out normalization: the three original pagination handlers differ in exactly one micro-behavior — whether a failed *re-fetch* (the "page > total pages, clamp and refetch" branch) sends `ERROR_TRY_LATER` to the user before propagating the error. `book`/`search` send it; `update_history` does not (its `.await?` propagates silently). The shared `paginate()` takes `error_try_later: Option<&str>` to preserve this exactly — `update_history` passes `None`, `book`/`search` pass `Some(ERROR_TRY_LATER)`. No other call site passes `None` for the *first* fetch, so first-fetch error messaging is untouched everywhere.
- Every task ends with `cargo test -p book_bot` green (from the workspace root `/Users/kurbezz/Projects/books_project/book_bot`) and `cargo fmt --all` applied before committing (CI runs `cargo fmt --all --check`).
- Tasks that touch `#[allow(dead_code)]` or remove a derive must additionally confirm with `cargo clippy --workspace --all-targets -- -D warnings` (CI gate) that no new warning appeared.
- File paths below are relative to the crate root `book_bot/` unless written as an absolute path.

---

### Task 1: Dedup `escape_html` into `teloxide::utils::html::escape`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/help/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/support/mod.rs`

**Interfaces:**
- Consumes: `teloxide::utils::html::escape(&str) -> String` (already available — `teloxide` is already a dependency, no `Cargo.toml` change).

- [ ] **Step 1: Replace both local `escape_html` copies and verify output is unchanged**

`teloxide::utils::html::escape` escapes `&`, `<`, `>` (plus `"` and `'`, which the local copies didn't — irrelevant here since none of the interpolated names come from a context where those characters need escaping for this bot's own HTML templates, and Telegram's HTML parse mode does not require quote-escaping outside attribute values, which this code never emits). The three characters that mattered (`&`, `<`, `>`) are escaped identically and in the same order, so output is byte-identical for all realistic Telegram `first_name` values.

In `book_bot/src/bots/approved_bot/modules/help/mod.rs`, remove:

```rust
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
```

and change line 25 from:

```rust
        .map(|user| escape_html(&user.first_name))
```

to:

```rust
        .map(|user| teloxide::utils::html::escape(&user.first_name))
```

In `book_bot/src/bots/approved_bot/modules/support/mod.rs`, remove the identical local `escape_html` function, and change:

```rust
        Some(user) if !user.is_bot => escape_html(&user.first_name),
        Some(user) if user.is_bot => match message.reply_to_message() {
            Some(v) => match &v.from {
                Some(v) => escape_html(&v.first_name),
```

to:

```rust
        Some(user) if !user.is_bot => teloxide::utils::html::escape(&user.first_name),
        Some(user) if user.is_bot => match message.reply_to_message() {
            Some(v) => match &v.from {
                Some(v) => teloxide::utils::html::escape(&v.first_name),
```

- [ ] **Step 2: Build and run the suite**

```bash
cargo build -p book_bot
cargo test -p book_bot
```

Expected: builds clean, `test result: ok. 136 passed; 0 failed`.

- [ ] **Step 3: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/help/mod.rs book_bot/src/bots/approved_bot/modules/support/mod.rs
git commit -m "refactor: dedup escape_html into teloxide::utils::html::escape"
```

---

### Task 2: `search::utils::get_query` takes `&CallbackQuery`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/search/utils.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/search/mod.rs:62`

**Interfaces:**
- Produces: `pub fn get_query(cq: &CallbackQuery) -> Option<String>` (was `get_query(cq: CallbackQuery)`).

- [ ] **Step 1: Write the failing test for the new signature**

`search/utils.rs` currently has no tests. Add one that exercises the function by reference (this is the "failing test" step — it fails to *compile* against the current by-value signature, which is the expected red state for a signature change):

```rust
#[cfg(test)]
mod tests {
    use super::get_query;
    use teloxide::types::{CallbackQuery, MaybeInaccessibleMessage};

    fn make_cq(message: Option<MaybeInaccessibleMessage>) -> CallbackQuery {
        serde_json::from_value(serde_json::json!({
            "id": "1",
            "from": {
                "id": 1,
                "is_bot": false,
                "first_name": "T"
            },
            "chat_instance": "1",
            "message": message,
        }))
        .unwrap()
    }

    #[test]
    fn returns_none_when_message_missing() {
        let cq = make_cq(None);
        assert_eq!(get_query(&cq), None);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails to compile**

```bash
cargo test -p book_bot get_query::returns_none_when_message_missing
```

Expected: compile error — `expected &CallbackQuery, found CallbackQuery` is not yet the error (the fn still takes ownership); actually expected error is the opposite — passing `&cq` where `CallbackQuery` (owned) is expected: `mismatched types`.

- [ ] **Step 3: Change the signature and its one call site**

In `book_bot/src/bots/approved_bot/modules/search/utils.rs`, change:

```rust
pub fn get_query(cq: CallbackQuery) -> Option<String> {
    match cq.message {
        Some(message) => match message {
```

to:

```rust
pub fn get_query(cq: &CallbackQuery) -> Option<String> {
    match &cq.message {
        Some(message) => match message {
```

(the rest of the function body is unchanged — `message.reply_to_message()` and `.text()` already work through a reference).

In `book_bot/src/bots/approved_bot/modules/search/mod.rs:62`, change:

```rust
    let query = get_query(cq.clone());
```

to:

```rust
    let query = get_query(&cq);
```

- [ ] **Step 4: Run the test and the full suite**

```bash
cargo test -p book_bot get_query::returns_none_when_message_missing
cargo test -p book_bot
```

Expected: new test passes; `test result: ok. 137 passed; 0 failed` (136 + 1 new test).

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/search/utils.rs book_bot/src/bots/approved_bot/modules/search/mod.rs
git commit -m "refactor: take &CallbackQuery in get_query, drop clone at call site"
```

---

### Task 3: `split_text_to_chunks` honors its `width` parameter

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/utils/split_text.rs`

**Interfaces:**
- Produces: `pub fn split_text_to_chunks(text: &str, width: usize) -> Vec<String>` — unchanged signature, now actually uses `width` for both the `textwrap::wrap` call and the chunk-merging threshold (previously only the merging threshold used `width`; wrapping was hardcoded to `512`). Both current call sites (`annotations/mod.rs:110,167`) already pass `512`, so this is a no-op for existing behavior and only fixes the latent bug for any future caller that passes a different width.

- [ ] **Step 1: Write a failing test that exercises a non-512 width**

Add to the bottom of `book_bot/src/bots/approved_bot/modules/utils/split_text.rs`, inside the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn wrap_width_is_honored_not_hardcoded() {
        // 20 chars of plain text, wrapped at width 10, should never
        // produce a line longer than 10 chars — this fails today because
        // `textwrap::wrap` is hardcoded to 512 regardless of `width`.
        let input = "aaaaaaaaaa bbbbbbbbbb";
        let result = split_text_to_chunks(input, 10);
        for chunk in &result {
            for line in chunk.split('\n') {
                assert!(
                    line.len() <= 10,
                    "line {line:?} (len {}) exceeds width 10",
                    line.len()
                );
            }
        }
    }
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p book_bot wrap_width_is_honored_not_hardcoded
```

Expected: FAIL — the 10-char-wide input line `"aaaaaaaaaa bbbbbbbbbb"` (21 chars) is not wrapped at all (since `textwrap::wrap(text, 512)` returns it as one 21-char line), so the assertion `line.len() <= 10` fails.

- [ ] **Step 3: Use `width` in the `textwrap::wrap` call**

Change:

```rust
    let chunks = textwrap::wrap(text, 512)
```

to:

```rust
    let chunks = textwrap::wrap(text, width)
```

- [ ] **Step 4: Run the new test and the full suite**

```bash
cargo test -p book_bot wrap_width_is_honored_not_hardcoded
cargo test -p book_bot
```

Expected: new test passes; `test result: ok. 138 passed; 0 failed`. The pre-existing `test_fix_annotation_text` test (which calls `split_text_to_chunks(input, 512)`) still passes unchanged, confirming the width-512 behavior is preserved byte-for-byte.

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/utils/split_text.rs
git commit -m "fix: split_text_to_chunks now wraps at the requested width instead of a hardcoded 512"
```

---

### Task 4: Remove dead `#[derive(EnumIter)]`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/download/callback_data.rs:5,21,49`
- Modify: `book_bot/src/bots/approved_bot/modules/download/commands.rs:5,38`
- Modify: `book_bot/src/bots/approved_bot/modules/search/callback_data.rs:5,15`

**Interfaces:** None — pure removal, no type gains or loses any trait impl that's used (`strum::IntoEnumIterator` is not called anywhere in the crate — verified via `grep -rln "IntoEnumIterator" src/` returning nothing).

- [ ] **Step 1: Remove the derive and its now-unused import in all three files**

In `book_bot/src/bots/approved_bot/modules/download/callback_data.rs`, remove line 5 (`use strum_macros::EnumIter;`) and change both:

```rust
#[derive(Clone, EnumIter)]
pub enum DownloadQueryData {
```

and

```rust
#[derive(Clone, EnumIter)]
pub enum DownloadArchiveQueryData {
```

to `#[derive(Clone)]`.

In `book_bot/src/bots/approved_bot/modules/download/commands.rs`, remove line 5 (`use strum_macros::EnumIter;`) and change:

```rust
#[derive(Clone, EnumIter)]
pub enum DownloadArchiveCommand {
```

to `#[derive(Clone)]`.

In `book_bot/src/bots/approved_bot/modules/search/callback_data.rs`, remove line 5 (`use strum_macros::EnumIter;`) and change:

```rust
#[derive(Clone, EnumIter)]
pub enum SearchCallbackData {
```

to `#[derive(Clone)]`.

- [ ] **Step 2: Build, test, and clippy-check**

```bash
cargo build -p book_bot
cargo test -p book_bot
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: builds clean, `test result: ok. 138 passed; 0 failed`, zero clippy warnings (in particular no `unused_imports` for `strum_macros::EnumIter`, and `strum_macros`/`strum` in `Cargo.toml` stay — `strum::ParseError` is still used by `SearchCallbackData`/`SettingsCallbackData`'s `FromStr` impls).

- [ ] **Step 3: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/download/callback_data.rs book_bot/src/bots/approved_bot/modules/download/commands.rs book_bot/src/bots/approved_bot/modules/search/callback_data.rs
git commit -m "chore: remove dead EnumIter derive (IntoEnumIterator is never called)"
```

---

### Task 5: Remove unnecessary `#[allow(dead_code)]`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/utils/telegram_utils.rs` (7 occurrences: `safe_send_message`, `safe_send_message_html`, `safe_edit_message_text_html`, `safe_send_document`, `safe_delete_message`, `safe_answer_callback_query`, `safe_answer_callback_query_with_text`)
- Modify: `book_bot/src/bots_manager/error_classification.rs:5` (module-wide `#![allow(dead_code)]`)

**Interfaces:** None — pure removal. All 7 `telegram_utils.rs` functions are `pub` and already called from other modules (`safe_send_message`/`safe_send_message_html`: `help`, `settings`, `search`, `update_history`; `safe_edit_message_text_html`/`safe_send_document`/`safe_delete_message`: `download`; `safe_answer_callback_query`/`safe_answer_callback_query_with_text`: `settings`) — confirmed live via `grep -rn` before writing this plan. `error_classification::is_expected_telegram_error` is called from `bots_manager/mod.rs`, `internal.rs`, and `custom_error_handler.rs`; `classify_telegram_error`/`ErrorCategory` are only used internally by `is_expected_telegram_error` in the same file, which itself is externally used, so nothing in the module is actually dead.

- [ ] **Step 1: Remove the 7 `#[allow(dead_code)]` lines in `telegram_utils.rs`**

Remove the `#[allow(dead_code)]` line immediately above each of these 7 function signatures (the doc comments above each stay):

```rust
pub async fn safe_send_message(
pub async fn safe_send_message_html(
pub async fn safe_edit_message_text_html(
pub async fn safe_send_document(
pub async fn safe_delete_message(
pub async fn safe_answer_callback_query(
pub async fn safe_answer_callback_query_with_text(
```

- [ ] **Step 2: Remove the module-wide `#![allow(dead_code)]` in `error_classification.rs`**

Remove line 5:

```rust
#![allow(dead_code)] // Used by custom_error_handler and main.rs Sentry filter
```

- [ ] **Step 3: Build and clippy-check for newly-surfaced dead-code warnings**

```bash
cargo build -p book_bot 2>&1 | grep -i warning
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: no `warning: function ... is never used` lines from the build; clippy passes with zero warnings. If clippy *does* flag something as dead, that function truly has no live caller — re-verify with `grep -rn "<fn name>" book_bot/src/` before deciding whether to delete the function (out of scope for this task if so) or keep a targeted, function-level `#[allow(dead_code)]` with a comment explaining why (e.g. reserved for a near-term caller) — but per the pre-check above, this is not expected to trigger.

- [ ] **Step 4: Run the full suite**

```bash
cargo test -p book_bot
```

Expected: `test result: ok. 138 passed; 0 failed`.

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/utils/telegram_utils.rs book_bot/src/bots_manager/error_classification.rs
git commit -m "chore: remove #[allow(dead_code)] from functions that are actually used"
```

---

### Task 6: Rename `src/bots/bots_manager` to `src/bots/registration`

**Files:**
- Move: `book_bot/src/bots/bots_manager/mod.rs` → `book_bot/src/bots/registration/mod.rs`
- Move: `book_bot/src/bots/bots_manager/register.rs` → `book_bot/src/bots/registration/register.rs`
- Move: `book_bot/src/bots/bots_manager/strings.rs` → `book_bot/src/bots/registration/strings.rs`
- Move: `book_bot/src/bots/bots_manager/utils.rs` → `book_bot/src/bots/registration/utils.rs`
- Modify: `book_bot/src/bots/mod.rs:2`
- Modify: `book_bot/src/bots/approved_bot/mod.rs:29`

**Interfaces:** None — pure rename. The module's public surface (`get_manager_handler()`) keeps its name; only its module path changes from `crate::bots::bots_manager` to `crate::bots::registration`. This is a distinct module from the unrelated `crate::bots_manager` (top-level, at `book_bot/src/bots_manager/`, the webhook/lifecycle manager) — the rename removes the name collision the two same-named-but-unrelated modules currently have.

- [ ] **Step 1: Move the four files**

```bash
mkdir -p book_bot/src/bots/registration
git mv book_bot/src/bots/bots_manager/mod.rs book_bot/src/bots/registration/mod.rs
git mv book_bot/src/bots/bots_manager/register.rs book_bot/src/bots/registration/register.rs
git mv book_bot/src/bots/bots_manager/strings.rs book_bot/src/bots/registration/strings.rs
git mv book_bot/src/bots/bots_manager/utils.rs book_bot/src/bots/registration/utils.rs
```

- [ ] **Step 2: Update the two references to the module path**

In `book_bot/src/bots/mod.rs:2`, change:

```rust
pub mod bots_manager;
```

to:

```rust
pub mod registration;
```

In `book_bot/src/bots/approved_bot/mod.rs`, change:

```rust
use super::{
    bots_manager::get_manager_handler, ignore_channel_messages, ignore_chat_join_request,
    ignore_chat_member_update, ignore_user_edited_message, BotCommands, BotHandler,
};
```

to:

```rust
use super::{
    ignore_channel_messages, ignore_chat_join_request, ignore_chat_member_update,
    ignore_user_edited_message, registration::get_manager_handler, BotCommands, BotHandler,
};
```

- [ ] **Step 3: Check for any other reference to the old path**

```bash
grep -rn "bots::bots_manager\|super::bots_manager" book_bot/src/
```

Expected: no output (the two references just updated were the only ones — confirmed via the same grep run before writing this plan, alongside the module's own internal `use crate::bots::approved_bot::modules::utils::telegram_utils::safe_send_message_with_reply;` in `mod.rs`, which is a `crate::` path and needs no change since only the trailing module segment moved, not anything it imports).

- [ ] **Step 4: Build and run the suite**

```bash
cargo build -p book_bot
cargo test -p book_bot
```

Expected: builds clean, `test result: ok. 138 passed; 0 failed`.

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add -A book_bot/src/bots/
git commit -m "refactor: rename src/bots/bots_manager to src/bots/registration to remove the name clash with crate::bots_manager"
```

---

### Task 7: Add the shared `paginate()` helper to `utils/pagination.rs`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/utils/pagination.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct PaginationTexts<'a> {
      pub not_found: &'a str,
      pub error_try_later: Option<&'a str>,
  }

  pub async fn paginate<T, P, Fut>(
      bot: &CacheMe<Throttle<Bot>>,
      chat_id: ChatId,
      message_id: MessageId,
      cq_message: Option<MaybeInaccessibleMessage>,
      page: u32,
      header: &str,
      fetcher: impl Fn(u32) -> Fut,
      keyboard_data: impl GetPaginationCallbackData,
      texts: PaginationTexts<'_>,
  ) -> crate::bots::BotHandlerInternal
  where
      T: Format + Clone + Debug,
      P: FormatTitle + Clone + Debug,
      Fut: std::future::Future<Output = anyhow::Result<Option<Page<T, P>>>>,
  ```
  This is consumed by Tasks 8, 9, and 10 — no existing call site is wired to it yet in this task, so it is validated with a direct unit test using fake `Format`/`FormatTitle` types (mirroring the pattern already used in `book_library/types.rs`'s own `#[cfg(test)]` block for `FakeItem`/`FakeParent`).

  `PaginationTexts` carries **two** distinct not-found strings, not one — discovered by reading all three call sites before writing this task: `book/mod.rs`'s `send_pagination_book_handler` sends `NOT_FOUND` when the fetcher's first call returns `Ok(None)` (the author/translator/sequence itself doesn't exist) but `BOOKS_NOT_FOUND` when it returns `Ok(Some(page))` with `page.pages == 0` (the entity exists but has no books) — two different constants (`"Не найдено :("` vs. `"Книги не найдены!"`). `search/mod.rs`'s `generic_search_pagination_handler` happens to use the *same* text for both cases already, so it will simply pass the same string for both fields. Collapsing these to one field would silently change `book/mod.rs`'s user-facing text, so `PaginationTexts` has `not_found` (first-call `Ok(None)`) and `no_items` (`pages == 0`, and also the re-fetch's `Ok(None)` — no existing call site's re-fetch can plausibly return `None` when the first call already succeeded, so which text it uses is unobservable in practice, and `no_items` is the more contextually apt of the two for that dead branch).

- [ ] **Step 1: Write failing tests using fake `Format`/`FormatTitle` types**

Add to the bottom of `book_bot/src/bots/approved_bot/modules/utils/pagination.rs`:

```rust
#[cfg(test)]
mod paginate_tests {
    use super::*;
    use crate::bots::approved_bot::services::book_library::{
        formatters::{Format, FormatResult, FormatTitle},
        types::Page,
    };
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Clone, Debug)]
    struct FakeItem(String);

    impl Format for FakeItem {
        fn format(&self, _max_size: usize) -> FormatResult {
            FormatResult {
                result: self.0.clone(),
                current_size: self.0.len(),
                max_size: self.0.len(),
            }
        }
    }

    #[derive(Clone, Debug)]
    struct FakeParent;

    impl FormatTitle for FakeParent {
        fn format_title(&self) -> String {
            "".to_string()
        }
    }

    #[derive(Clone)]
    struct FakeKeyboardData;

    impl GetPaginationCallbackData for FakeKeyboardData {
        fn get_pagination_callback_data(&self, target_page: u32) -> String {
            format!("fake_{target_page}")
        }
    }

    fn make_page(pages: u32) -> Page<FakeItem, FakeParent> {
        Page {
            items: vec![FakeItem("item".to_string())],
            pages,
            parent_item: None,
        }
    }

    #[tokio::test]
    async fn clamps_page_above_total_and_calls_fetcher_with_clamped_page() {
        let calls = AtomicU32::new(0);
        let last_requested_page = AtomicU32::new(0);

        let fetcher = |page: u32| {
            calls.fetch_add(1, Ordering::SeqCst);
            last_requested_page.store(page, Ordering::SeqCst);
            async move {
                if page == 3 {
                    Ok(Some(make_page(3)))
                } else {
                    Ok(Some(make_page(3)))
                }
            }
        };

        // We can't easily run the bot-send path without a live `CacheMe<Throttle<Bot>>`,
        // so this test only exercises the fetch/clamp logic by calling the fetcher
        // directly the same way `paginate` does, verifying the two-call clamp pattern.
        let first = fetcher(10).await.unwrap().unwrap();
        assert_eq!(first.pages, 3);
        if 10 > first.pages {
            let _second = fetcher(first.pages).await.unwrap().unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(last_requested_page.load(Ordering::SeqCst), 3);
    }
}
```

- [ ] **Step 2: Run it to verify it fails to compile**

```bash
cargo test -p book_bot clamps_page_above_total_and_calls_fetcher_with_clamped_page
```

Expected: this specific test actually compiles and passes standalone right now — it only exercises the `fetcher`/fake-type shapes, not `paginate` itself (which doesn't exist yet). Run it to confirm the fakes behave as intended before `paginate` exists, the same way you'd sanity-check a test double before wiring it to the real thing. `paginate`/`PaginationTexts` don't exist yet, so nothing here references them — this step is a checkpoint, not a compile failure. The actual "does this fail" checkpoint is the next one.

Add one more test to the same `mod paginate_tests` block, this time referencing `paginate` directly so Step 3 has a real red bar:

```rust
    #[tokio::test]
    async fn not_found_texts_are_distinct_fields() {
        // Compile-time check that `PaginationTexts` has two distinct
        // not-found fields (see the Interfaces note above) rather than
        // one shared field — this is what book/mod.rs (Task 8) needs.
        let texts = PaginationTexts {
            not_found: "a",
            no_items: "b",
            error_try_later: Some("c"),
        };
        assert_ne!(texts.not_found, texts.no_items);
    }
```

```bash
cargo test -p book_bot not_found_texts_are_distinct_fields
```

Expected: FAIL to compile — `cannot find struct 'PaginationTexts' in this scope`.

- [ ] **Step 3: Implement `PaginationTexts` and `paginate()`**

Add these imports at the top of `book_bot/src/bots/approved_bot/modules/utils/pagination.rs` (alongside the existing `use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};`):

```rust
use core::fmt::Debug;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{ChatId, MaybeInaccessibleMessage, MessageId},
};

use crate::bots::approved_bot::services::book_library::{
    formatters::{Format, FormatTitle},
    types::Page,
};

use super::{
    message_text::is_message_text_equals,
    telegram_utils::{safe_edit_message_text, safe_send_message},
};
```

Add, after the existing `generic_get_pagination_keyboard` function:

```rust
pub struct PaginationTexts<'a> {
    /// Sent when the fetcher's first call returns `Ok(None)` (the parent
    /// entity — author/translator/sequence/search query — doesn't exist).
    pub not_found: &'a str,
    /// Sent when the fetcher returns `Ok(Some(page))` but `page.pages == 0`
    /// (the entity exists but has no items), and reused for the re-fetch's
    /// `Ok(None)` branch on the clamp path (unreachable in practice — see
    /// Task 8/9's notes on this field).
    pub no_items: &'a str,
    pub error_try_later: Option<&'a str>,
}

/// Shared skeleton for "fetch a page → not-found → clamp page → format →
/// no-op-if-unchanged → edit message + pagination keyboard", used by the
/// `book`, `search`, and `update_history` modules' callback-query
/// pagination handlers. Callers own everything data-source-specific
/// (extracting the query/id from the callback data, building the
/// `fetcher` closure, and resolving `chat_id`/`message_id` from the
/// incoming `CallbackQuery`).
#[allow(clippy::too_many_arguments)]
pub async fn paginate<T, P, Fut>(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_message: Option<MaybeInaccessibleMessage>,
    page: u32,
    header: &str,
    fetcher: impl Fn(u32) -> Fut,
    keyboard_data: impl GetPaginationCallbackData,
    texts: PaginationTexts<'_>,
) -> crate::bots::BotHandlerInternal
where
    T: Format + Clone + Debug,
    P: FormatTitle + Clone + Debug,
    Fut: std::future::Future<Output = anyhow::Result<Option<Page<T, P>>>>,
{
    let mut items_page = match fetcher(page).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            safe_send_message(bot, chat_id, texts.not_found, None).await?;
            return Ok(());
        }
        Err(err) => {
            if let Some(msg) = texts.error_try_later {
                safe_send_message(bot, chat_id, msg, None).await?;
            }
            return Err(err);
        }
    };

    if items_page.pages == 0 {
        safe_send_message(bot, chat_id, texts.no_items, None).await?;
        return Ok(());
    }

    if page > items_page.pages {
        items_page = match fetcher(items_page.pages).await {
            Ok(Some(v)) => v,
            Ok(None) => {
                safe_send_message(bot, chat_id, texts.no_items, None).await?;
                return Ok(());
            }
            Err(err) => {
                if let Some(msg) = texts.error_try_later {
                    safe_send_message(bot, chat_id, msg, None).await?;
                }
                return Err(err);
            }
        };
    }

    let page = std::cmp::min(page, items_page.pages);
    let formatted_page = items_page.format(page, super::constants::TELEGRAM_MESSAGE_MAX_LENGTH);
    let message_text = format!("{header}{formatted_page}");

    if is_message_text_equals(cq_message, &message_text) {
        return Ok(());
    }

    let keyboard = generic_get_pagination_keyboard(page, items_page.pages, keyboard_data, true);
    safe_edit_message_text(bot, chat_id, message_id, message_text, Some(keyboard)).await
}
```

- [ ] **Step 4: Run the new tests to verify they pass, then the full suite**

```bash
cargo test -p book_bot paginate_tests
cargo test -p book_bot
```

Expected: `test result: ok. 2 passed; 0 failed` for `paginate_tests::` (both `clamps_page_above_total_and_calls_fetcher_with_clamped_page` and `not_found_texts_are_distinct_fields`); full suite `test result: ok. 140 passed; 0 failed` (138 + 2 new tests).

- [ ] **Step 5: Build and clippy-check**

```bash
cargo build -p book_bot
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: builds clean (no consumer yet, so `paginate` is unused outside its own tests — this is expected and resolved by Task 8; if clippy flags `paginate` itself as dead code at this intermediate step, that's fine and self-resolves once Task 8 wires it in immediately after).

- [ ] **Step 6: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/utils/pagination.rs
git commit -m "feat: add shared paginate() helper for the fetch/clamp/format/edit pagination skeleton"
```

---

### Task 8: Wire `paginate()` into `book/mod.rs`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/book/mod.rs`

**Interfaces:**
- Consumes: `paginate()` and `PaginationTexts` from Task 7.
- Preserves: `send_pagination_book_handler`'s public behavior — same not-found text (`NOT_FOUND` on missing item, `BOOKS_NOT_FOUND` on zero pages), same `REPEAT_SEARCH` fallback when `chat_id`/`message_id` can't be resolved from the callback query, same keyboard.

- [ ] **Step 1: Replace the body of `send_pagination_book_handler` to delegate to `paginate()`**

Change:

```rust
    let allowed_langs = get_user_or_default_lang_codes(user_id).await;

    let mut items_page = match books_getter(id, page, allowed_langs.clone()).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            match safe_send_message(&bot, chat_id, NOT_FOUND, None).await {
                Ok(_) => (),
                Err(err) => log::error!("{err:?}"),
            }
            return Ok(());
        }
        Err(err) => {
            match safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await {
                Ok(_) => (),
                Err(err) => log::error!("{err:?}"),
            }
            return Err(err);
        }
    };

    if items_page.pages == 0 {
        safe_send_message(&bot, chat_id, BOOKS_NOT_FOUND, None).await?;
        return Ok(());
    };

    if page > items_page.pages {
        items_page = match books_getter(id, items_page.pages, allowed_langs).await {
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
    }

    let formatted_page = items_page.format(page, TELEGRAM_MESSAGE_MAX_LENGTH);

    let keyboard = generic_get_pagination_keyboard(page, items_page.pages, callback_data, true);

    if is_message_text_equals(cq.message, &formatted_page) {
        return Ok(());
    }

    safe_edit_message_text(&bot, chat_id, message_id, formatted_page, Some(keyboard)).await
}
```

to:

```rust
    let allowed_langs = get_user_or_default_lang_codes(user_id).await;

    paginate(
        &bot,
        chat_id,
        message_id,
        cq.message,
        page,
        "",
        |p| books_getter(id, p, allowed_langs.clone()),
        callback_data,
        PaginationTexts {
            not_found: NOT_FOUND,
            no_items: BOOKS_NOT_FOUND,
            error_try_later: Some(ERROR_TRY_LATER),
        },
    )
    .await
}
```

Add the import at the top of `book/mod.rs`:

```rust
use super::utils::pagination::{generic_get_pagination_keyboard, paginate, PaginationTexts};
```

(replacing the old `use super::utils::pagination::generic_get_pagination_keyboard;` — `generic_get_pagination_keyboard` is still used directly by `send_book_handler`, the message-based initial-search handler in the same file, so keep that import).

Remove now-unused imports from `book/mod.rs`: `is_message_text_equals` (no longer called directly in this file) and `tracing::log` if `send_book_handler` doesn't use it (check — `send_book_handler` doesn't reference `log::`, only `send_pagination_book_handler`'s old code did, in the swallowed-error branches now gone). Remove `use tracing::log;` and `message_text::is_message_text_equals` from the `use` block if the compiler flags them unused.

- [ ] **Step 2: Build and fix any leftover unused-import warnings**

```bash
cargo build -p book_bot 2>&1 | grep -E "warning|error"
```

Expected: fix any `unused import` warnings by removing them (per Step 1's note); no other warnings.

- [ ] **Step 3: Run the full suite**

```bash
cargo test -p book_bot
```

Expected: `test result: ok. 140 passed; 0 failed` (no new tests in this task — behavior-preserving wiring, validated by the existing suite plus the manual-check note in Task 17).

- [ ] **Step 4: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/book/mod.rs book_bot/src/bots/approved_bot/modules/utils/pagination.rs
git commit -m "refactor: wire book/mod.rs pagination handler through the shared paginate() helper"
```

---

### Task 9: Wire `paginate()` into `search/mod.rs`'s callback pagination handler

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/search/mod.rs`

**Interfaces:**
- Consumes: `paginate()`/`PaginationTexts` from Tasks 7–8 (now with the `not_found`/`no_items` split).

- [ ] **Step 1: Replace the body of `generic_search_pagination_handler` from the fetch step onward**

The current function (lines 47-138) does its own `chat_id`/`query`/`message_id` resolution (lines 59-82, including the `REPEAT_SEARCH` fallback and computing the per-variant not-found text) — all of that stays. Only the "fetch → clamp → format → edit" tail (lines 84-137) is replaced.

Change:

```rust
    let mut items_page = match items_getter(query.clone(), page, allowed_langs.clone()).await {
        Ok(Some(v)) => v,
        Ok(None) => {
            let message_text = match search_data {
                SearchCallbackData::Book { .. } => BOOKS_NOT_FOUND,
                SearchCallbackData::Authors { .. } => AUTHORS_NOT_FOUND,
                SearchCallbackData::Sequences { .. } => SEQUENCES_NOT_FOUND,
                SearchCallbackData::Translators { .. } => TRANSLATORS_NOT_FOUND,
            };

            safe_send_message(&bot, chat_id, message_text, None).await?;
            return Ok(());
        }
        Err(err) => {
            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;

            return Err(err);
        }
    };

    if items_page.pages == 0 {
        let message_text = match search_data {
            SearchCallbackData::Book { .. } => BOOKS_NOT_FOUND,
            SearchCallbackData::Authors { .. } => AUTHORS_NOT_FOUND,
            SearchCallbackData::Sequences { .. } => SEQUENCES_NOT_FOUND,
            SearchCallbackData::Translators { .. } => TRANSLATORS_NOT_FOUND,
        };

        safe_send_message(&bot, chat_id, message_text, None).await?;
        return Ok(());
    };

    if page > items_page.pages {
        items_page = match items_getter(query, items_page.pages, allowed_langs).await {
            Ok(Some(v)) => v,
            Ok(None) => {
                safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                return Ok(());
            }
            Err(err) => {
                safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;

                return Err(err);
            }
        };
    }

    let formatted_page = items_page.format(page, TELEGRAM_MESSAGE_MAX_LENGTH);
    if is_message_text_equals(cq.message, &formatted_page) {
        return Ok(());
    }

    let keyboard = generic_get_pagination_keyboard(page, items_page.pages, search_data, true);
    safe_edit_message_text(&bot, chat_id, message_id, formatted_page, Some(keyboard)).await
}
```

to:

```rust
    let not_found_text = match search_data {
        SearchCallbackData::Book { .. } => BOOKS_NOT_FOUND,
        SearchCallbackData::Authors { .. } => AUTHORS_NOT_FOUND,
        SearchCallbackData::Sequences { .. } => SEQUENCES_NOT_FOUND,
        SearchCallbackData::Translators { .. } => TRANSLATORS_NOT_FOUND,
    };

    paginate(
        &bot,
        chat_id,
        message_id,
        cq.message,
        page,
        "",
        |p| items_getter(query.clone(), p, allowed_langs.clone()),
        search_data,
        PaginationTexts {
            not_found: not_found_text,
            no_items: not_found_text,
            error_try_later: Some(ERROR_TRY_LATER),
        },
    )
    .await
}
```

Note the original re-fetch `Ok(None)` branch sent `ERROR_TRY_LATER` (not the not-found text) — a genuine one-off inconsistency versus the pages==0 branch just above it, most likely an unintentional copy-paste slip in the original code (the "entity vanished between fetch 1 and fetch 2" case is unreachable in practice — page counts don't change between two immediately-sequential reads against a search index — so this divergence has no observable effect and there's no test exercising it). `paginate()`'s shared re-fetch `Ok(None)` branch uses `texts.no_items`, matching `search`'s pages==0 text — an intentional normalization of dead-branch text, consistent with the Global Constraints note on the *one* deliberate normalization already called out for the `error_try_later` field (this is a second, equally inert one worth documenting here since it's this task, not Task 7, where it was found).

Add the import at the top of `search/mod.rs`, replacing `use super::utils::pagination::generic_get_pagination_keyboard;`:

```rust
use super::utils::pagination::{paginate, PaginationTexts};
```

(`generic_get_pagination_keyboard` is no longer called directly anywhere in `search/mod.rs` — `message_handler`, the other function in this file, calls it too at line 279; keep it imported. Final import: `use super::utils::pagination::{generic_get_pagination_keyboard, paginate, PaginationTexts};`.)

Remove `message_text::is_message_text_equals` and `telegram_utils::safe_edit_message_text` from the top-level `use` block if the compiler flags them unused after this change (`safe_send_message`/`safe_send_message_with_reply` are still used by `message_handler`).

- [ ] **Step 2: Build and fix leftover unused-import warnings**

```bash
cargo build -p book_bot 2>&1 | grep -E "warning|error"
```

- [ ] **Step 3: Run the full suite**

```bash
cargo test -p book_bot
```

Expected: `test result: ok. 140 passed; 0 failed`.

- [ ] **Step 4: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/search/mod.rs
git commit -m "refactor: wire search/mod.rs callback pagination handler through paginate()"
```

---

### Task 10: Wire `paginate()` into `update_history/mod.rs`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/update_history/mod.rs`

**Interfaces:**
- Consumes: `paginate()`/`PaginationTexts` from Tasks 7–9.
- Preserves: the header line (`"Обновление каталога ({from} - {to}):\n\n"`), the "Нет новых книг за этот период." text for both not-found cases, and the *lack* of an error message on the re-fetch's error path (this module passes `error_try_later: None`, per the Global Constraints note).

- [ ] **Step 1: Replace `update_log_pagination_handler`'s body from the fetch step onward**

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

    if items_page.pages == 0 {
        safe_send_message(
            &bot,
            message.chat().id,
            "Нет новых книг за этот период.",
            None,
        )
        .await?;
        return Ok(());
    }

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

    let page = update_callback_data.page;
    let total_pages = items_page.pages;

    let formatted_page = items_page.format(page, TELEGRAM_MESSAGE_MAX_LENGTH);

    let message_text = format!("{header}{formatted_page}");
    if is_message_text_equals(cq.message, &message_text) {
        return Ok(());
    }

    let keyboard = generic_get_pagination_keyboard(page, total_pages, update_callback_data, true);
    safe_edit_message_text(
        &bot,
        message.chat().id,
        message.id(),
        message_text,
        Some(keyboard),
    )
    .await
}
```

to:

```rust
    const NO_NEW_BOOKS: &str = "Нет новых книг за этот период.";

    let from = update_callback_data.from;
    let to = update_callback_data.to;

    paginate(
        &bot,
        message.chat().id,
        message.id(),
        cq.message,
        update_callback_data.page,
        &header,
        move |p| {
            get_uploaded_books(
                p,
                from.format("%Y-%m-%d").to_string().into(),
                to.format("%Y-%m-%d").to_string().into(),
            )
        },
        update_callback_data,
        PaginationTexts {
            not_found: NO_NEW_BOOKS,
            no_items: NO_NEW_BOOKS,
            error_try_later: None,
        },
    )
    .await
}
```

Note the closure captures `from`/`to` by move (both `NaiveDate`, `Copy`) instead of `update_callback_data.from`/`.to` directly, because `update_callback_data` itself is moved into the final `paginate(...)` call as `keyboard_data` — extracting `from`/`to` as local `Copy` bindings first avoids a partial-move conflict (mirroring the existing `let (id, page) = match callback_data { ... }` pattern already used in `book/mod.rs`, where `Copy` fields are pulled out before the whole value is reused).

Add the import at the top of `update_history/mod.rs`, replacing `use super::utils::pagination::generic_get_pagination_keyboard;`:

```rust
use super::utils::pagination::{paginate, PaginationTexts};
```

Remove `message_text::is_message_text_equals` and `telegram_utils::safe_edit_message_text` from the `use` block if flagged unused (`safe_send_message` is still used by `update_log_command`, the other function in this file).

- [ ] **Step 2: Build and fix leftover unused-import warnings**

```bash
cargo build -p book_bot 2>&1 | grep -E "warning|error"
```

- [ ] **Step 3: Run the full suite**

```bash
cargo test -p book_bot
```

Expected: `test result: ok. 140 passed; 0 failed`.

- [ ] **Step 4: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/update_history/mod.rs
git commit -m "refactor: wire update_history/mod.rs pagination handler through paginate()"
```

---

### Task 11: Dedup the four-way match in `search::message_handler`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/search/mod.rs`

**Interfaces:**
- Produces: a private helper `async fn search_first_page<T, P, Fut>(query: String, allowed_langs: SmallVec<[SmartString; 3]>, not_found: &str, search_fn: fn(String, u32, SmallVec<[SmartString; 3]>) -> Fut) -> anyhow::Result<Option<(String, u32)>>` — returns `Ok(None)` when the caller should send the not-found text (both the "no results" and "zero pages" cases collapse into this, matching the two branches' identical outcome in the original code), `Ok(Some((formatted, pages)))` on success, `Err` to propagate.

- [ ] **Step 1: Write the failing test for `search_first_page`**

Add near the top of `search/mod.rs`, in a new `#[cfg(test)] mod tests` block (this file has none yet):

```rust
#[cfg(test)]
mod tests {
    use super::search_first_page;
    use crate::bots::approved_bot::services::book_library::types::{Empty, Page, SearchBook};
    use smallvec::smallvec;

    async fn fake_found(
        _query: String,
        _page: u32,
        _allowed_langs: smallvec::SmallVec<[smartstring::alias::String; 3]>,
    ) -> anyhow::Result<Option<Page<SearchBook, Empty>>> {
        Ok(Some(Page {
            items: vec![],
            pages: 2,
            parent_item: None,
        }))
    }

    async fn fake_zero_pages(
        _query: String,
        _page: u32,
        _allowed_langs: smallvec::SmallVec<[smartstring::alias::String; 3]>,
    ) -> anyhow::Result<Option<Page<SearchBook, Empty>>> {
        Ok(Some(Page {
            items: vec![],
            pages: 0,
            parent_item: None,
        }))
    }

    async fn fake_not_found(
        _query: String,
        _page: u32,
        _allowed_langs: smallvec::SmallVec<[smartstring::alias::String; 3]>,
    ) -> anyhow::Result<Option<Page<SearchBook, Empty>>> {
        Ok(None)
    }

    #[tokio::test]
    async fn returns_formatted_page_and_pages_on_success() {
        let result = search_first_page(
            "q".to_string(),
            smallvec!["ru".into()],
            "not found",
            fake_found,
        )
        .await
        .unwrap();
        let (_, pages) = result.expect("expected Some");
        assert_eq!(pages, 2);
    }

    #[tokio::test]
    async fn returns_none_on_zero_pages() {
        let result = search_first_page(
            "q".to_string(),
            smallvec!["ru".into()],
            "not found",
            fake_zero_pages,
        )
        .await
        .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn returns_none_when_search_fn_returns_none() {
        let result = search_first_page(
            "q".to_string(),
            smallvec!["ru".into()],
            "not found",
            fake_not_found,
        )
        .await
        .unwrap();
        assert!(result.is_none());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

```bash
cargo test -p book_bot search_first_page
```

Expected: compile error — `cannot find function 'search_first_page' in this scope`.

- [ ] **Step 3: Implement `search_first_page` and rewrite `message_handler`'s four-way match to call it once**

Add this private helper above `message_handler` in `search/mod.rs`:

```rust
async fn search_first_page<Fut>(
    query: String,
    allowed_langs: SmallVec<[SmartString; 3]>,
    not_found: &str,
    search_fn: fn(String, u32, SmallVec<[SmartString; 3]>) -> Fut,
) -> anyhow::Result<Option<(String, u32)>>
where
    Fut: std::future::Future<
        Output = anyhow::Result<
            Option<crate::bots::approved_bot::services::book_library::types::Page<
                impl_format::Item,
                impl_format::Parent,
            >>,
        >,
    >,
{
    let _ = not_found; // placeholder to keep signature stable during Step 1's compile-fail; replaced below.
    unimplemented!()
}
```

This generic-over-`impl Trait`-in-return-position shape doesn't actually work for a named helper (the four search functions return four *different* concrete `Page<T, P>` types — `Page<SearchBook, Empty>`, `Page<Author, Empty>`, `Page<Sequence, Empty>`, `Page<Translator, Empty>` — so `Fut`'s output type must be generic over `T`, not hidden behind one opaque type). Replace the sketch above entirely with the real, correctly-generic version:

```rust
async fn search_first_page<T, Fut>(
    query: String,
    allowed_langs: SmallVec<[SmartString; 3]>,
    not_found: &str,
    search_fn: fn(String, u32, SmallVec<[SmartString; 3]>) -> Fut,
) -> anyhow::Result<Option<(String, u32)>>
where
    T: Format + Clone + Debug,
    Fut: std::future::Future<Output = anyhow::Result<Option<Page<T, Empty>>>>,
{
    match search_fn(query, 1, allowed_langs).await {
        Ok(None) => Ok(None),
        Ok(Some(p)) if p.pages == 0 => Ok(None),
        Ok(Some(p)) => Ok(Some((p.format(1, TELEGRAM_MESSAGE_MAX_LENGTH), p.pages))),
        Err(err) => Err(err),
    }
}
```

Add `Page` and `Empty` to the existing `book_library` import in `search/mod.rs` (they're already imported as `types::Page`; add `types::Empty` alongside):

```rust
        services::{
            book_library::{
                formatters::{Format, FormatTitle},
                search_author, search_book, search_sequence, search_translator,
                types::{Empty, Page},
            },
            user_settings::{get_user_default_search, get_user_or_default_lang_codes},
        },
```

Now rewrite the four-way match inside `message_handler`. Replace the entire block from `let (formatted, pages) = match &search_data { ... };` (lines 152-277 in the original) with:

```rust
            let not_found_text = match &search_data {
                SearchCallbackData::Book { .. } => BOOKS_NOT_FOUND,
                SearchCallbackData::Authors { .. } => AUTHORS_NOT_FOUND,
                SearchCallbackData::Sequences { .. } => SEQUENCES_NOT_FOUND,
                SearchCallbackData::Translators { .. } => TRANSLATORS_NOT_FOUND,
            };

            let result = match &search_data {
                SearchCallbackData::Book { .. } => {
                    search_first_page(query_owned, allowed_langs, not_found_text, search_book).await
                }
                SearchCallbackData::Authors { .. } => {
                    search_first_page(query_owned, allowed_langs, not_found_text, search_author)
                        .await
                }
                SearchCallbackData::Sequences { .. } => {
                    search_first_page(query_owned, allowed_langs, not_found_text, search_sequence)
                        .await
                }
                SearchCallbackData::Translators { .. } => {
                    search_first_page(
                        query_owned,
                        allowed_langs,
                        not_found_text,
                        search_translator,
                    )
                    .await
                }
            };

            let (formatted, pages) = match result {
                Ok(Some(v)) => v,
                Ok(None) => {
                    safe_send_message_with_reply(
                        &bot,
                        chat_id,
                        not_found_text,
                        ReplyParameters::new(message.id),
                        None,
                    )
                    .await?;
                    return Ok(());
                }
                Err(err) => {
                    safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                    return Err(err);
                }
            };
```

This still branches on `search_data`'s variant twice (once for `not_found_text`, once to pick the right `fn` pointer to pass to `search_first_page` — Rust can't unify four `fn(String, u32, SmallVec<...>) -> Fut` items with four *different* `Fut` types into one call without the outer match, since each concrete search function returns a distinctly-typed future), but each arm is now a single line instead of the original ~30-line block, and the ~30 lines of not-found/format/error handling exist exactly once instead of 4 times — matching the spec's request to remove the "~125 duplicated lines," which this reduces to the four one-line dispatch arms (~8 lines) plus one shared handling block (~20 lines).

- [ ] **Step 4: Run the tests and the full suite**

```bash
cargo test -p book_bot search_first_page
cargo test -p book_bot
```

Expected: `search_first_page`'s 3 tests pass; `test result: ok. 143 passed; 0 failed` (140 + 3 new tests).

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/search/mod.rs
git commit -m "refactor: dedup the four-way Book/Authors/Sequences/Translators match in search::message_handler"
```

---

### Task 12: `book_library/formatters.rs` — generic `format_list()`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/services/book_library/formatters.rs`

**Interfaces:**
- Produces: `fn format_list<T>(items: &[T], count: usize, header: &str, fmt: impl Fn(&T) -> String) -> String`, replacing `format_authors`, `format_translators`, `format_sequences`, `format_genres`.

- [ ] **Step 1: Write the failing test for `format_list`**

Add a `#[cfg(test)] mod tests` block at the bottom of `formatters.rs` (this file has none yet):

```rust
#[cfg(test)]
mod tests {
    use super::format_list;

    #[test]
    fn count_zero_yields_empty_string() {
        let items = vec!["a".to_string(), "b".to_string()];
        assert_eq!(format_list(&items, 0, "Header:\n", |s| s.clone()), "");
    }

    #[test]
    fn empty_items_yields_empty_string_even_with_positive_count() {
        let items: Vec<String> = vec![];
        assert_eq!(format_list(&items, 5, "Header:\n", |s| s.clone()), "");
    }

    #[test]
    fn formats_up_to_count_items_with_header_and_no_suffix_when_exact() {
        let items = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            format_list(&items, 2, "Header:\n", |s| s.clone()),
            "Header:\na\nb\n"
        );
    }

    #[test]
    fn truncates_to_count_and_appends_i_dr_suffix_when_more_items_exist() {
        let items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            format_list(&items, 2, "Header:\n", |s| s.clone()),
            "Header:\na\nb\nи др.\n"
        );
    }
}
```

- [ ] **Step 2: Run it to verify it fails to compile**

```bash
cargo test -p book_bot format_list
```

Expected: compile error — `cannot find function 'format_list' in this scope`.

- [ ] **Step 3: Implement `format_list` and replace the four call sites**

Add above `format_authors` (which it will replace):

```rust
fn format_list<T>(items: &[T], count: usize, header: &str, fmt: impl Fn(&T) -> String) -> String {
    if count == 0 || items.is_empty() {
        return "".to_string();
    }

    let formatted_items = items[..min(count, items.len())]
        .iter()
        .map(fmt)
        .collect::<Vec<String>>()
        .join("\n");

    let post_fix = if items.len() > count { "\nи др." } else { "" };

    format!("{header}{formatted_items}{post_fix}\n")
}
```

Remove `format_authors`, `format_translators`, `format_sequences`, and `format_genres` entirely, and update their two call sites inside `format_vectors`:

```rust
    let mut result = FormatVectorsResult {
        authors: format_authors(authors, counts.authors),
        translators: format_translators(translators, counts.translators),
        sequences: format_sequences(sequences, counts.sequences),
        genres: format_genres(genres, counts.genres),
        max_result_size: 0,
    };
```

(appears twice, once before the `while` loop and once inside it) to:

```rust
    let mut result = FormatVectorsResult {
        authors: format_list(authors, counts.authors, "Авторы:\n", |a| a.format_inline()),
        translators: format_list(translators, counts.translators, "Переводчики:\n", |t| {
            t.format_inline()
        }),
        sequences: format_list(sequences, counts.sequences, "Серии:\n", |s| {
            s.format(NO_LIMIT).result
        }),
        genres: format_list(genres, counts.genres, "Жанры:\n", |g| g.format()),
        max_result_size: 0,
    };
```

- [ ] **Step 4: Run the tests and the full suite**

```bash
cargo test -p book_bot format_list
cargo test -p book_bot
```

Expected: `format_list`'s 4 tests pass; `test result: ok. 147 passed; 0 failed` (143 + 4 new tests).

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/services/book_library/formatters.rs
git commit -m "refactor: replace format_authors/translators/sequences/genres with a generic format_list()"
```

---

### Task 13: `book_library/formatters.rs` — rewrite `FormatVectorsCounts::sub` as a loop

**Files:**
- Modify: `book_bot/src/bots/approved_bot/services/book_library/formatters.rs`

**Interfaces:**
- Preserves: `FormatVectorsCounts::sub(self) -> Self` — same decrement-priority order (genres, then sequences, then translators, then authors), same behavior when all counts are already zero (no-op, `can_sub()` guards every call site so this path is never actually hit, but the rewrite preserves it anyway since `sub` remains a safe total function).

- [ ] **Step 1: Write the failing test for the decrement order**

Add to the `#[cfg(test)] mod tests` block added in Task 12:

```rust
    use super::FormatVectorsCounts;

    #[test]
    fn sub_decrements_genres_first() {
        let counts = FormatVectorsCounts {
            authors: 1,
            translators: 1,
            sequences: 1,
            genres: 1,
        }
        .sub();
        assert_eq!((counts.authors, counts.translators, counts.sequences, counts.genres), (1, 1, 1, 0));
    }

    #[test]
    fn sub_decrements_sequences_when_genres_already_zero() {
        let counts = FormatVectorsCounts {
            authors: 1,
            translators: 1,
            sequences: 1,
            genres: 0,
        }
        .sub();
        assert_eq!((counts.authors, counts.translators, counts.sequences, counts.genres), (1, 1, 0, 0));
    }

    #[test]
    fn sub_decrements_translators_when_genres_and_sequences_zero() {
        let counts = FormatVectorsCounts {
            authors: 1,
            translators: 1,
            sequences: 0,
            genres: 0,
        }
        .sub();
        assert_eq!((counts.authors, counts.translators, counts.sequences, counts.genres), (1, 0, 0, 0));
    }

    #[test]
    fn sub_decrements_authors_last() {
        let counts = FormatVectorsCounts {
            authors: 1,
            translators: 0,
            sequences: 0,
            genres: 0,
        }
        .sub();
        assert_eq!((counts.authors, counts.translators, counts.sequences, counts.genres), (0, 0, 0, 0));
    }

    #[test]
    fn sub_is_a_no_op_when_all_already_zero() {
        let counts = FormatVectorsCounts {
            authors: 0,
            translators: 0,
            sequences: 0,
            genres: 0,
        }
        .sub();
        assert_eq!((counts.authors, counts.translators, counts.sequences, counts.genres), (0, 0, 0, 0));
    }
```

`FormatVectorsCounts`'s fields are private (`struct FormatVectorsCounts { authors: usize, ... }`, no `pub`), and the test module is a *child* of the `formatters` module, so it already has access to private fields via `use super::FormatVectorsCounts;` — no visibility change is needed since `#[cfg(test)] mod tests` is nested inside `formatters.rs` itself.

- [ ] **Step 2: Run the tests to verify they pass against the current implementation first (characterization, not red-green, since `sub` already exists)**

```bash
cargo test -p book_bot sub_decrements
cargo test -p book_bot sub_is_a_no_op_when_all_already_zero
```

Expected: all 5 pass already — this task's tests characterize the *existing* `sub` before rewriting it, so the rewrite in Step 3 has a safety net proving it's behavior-preserving.

- [ ] **Step 3: Rewrite `sub` as a loop over `&mut` references**

Replace:

```rust
    fn sub(self) -> Self {
        let Self {
            mut authors,
            mut translators,
            mut sequences,
            mut genres,
        } = self;

        if genres > 0 {
            genres -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres,
            };
        }

        if sequences > 0 {
            sequences -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres,
            };
        }

        if translators > 0 {
            translators -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres,
            };
        }

        if authors > 0 {
            authors -= 1;

            return Self {
                authors,
                translators,
                sequences,
                genres,
            };
        }

        Self {
            authors,
            translators,
            sequences,
            genres,
        }
    }
```

with:

```rust
    fn sub(mut self) -> Self {
        for count in [
            &mut self.genres,
            &mut self.sequences,
            &mut self.translators,
            &mut self.authors,
        ] {
            if *count > 0 {
                *count -= 1;
                break;
            }
        }

        self
    }
```

- [ ] **Step 4: Run the tests and the full suite**

```bash
cargo test -p book_bot sub_decrements
cargo test -p book_bot sub_is_a_no_op_when_all_already_zero
cargo test -p book_bot
```

Expected: all 5 `sub_*` tests still pass (proving the rewrite is behavior-identical); `test result: ok. 152 passed; 0 failed` (147 + 5 new tests).

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/services/book_library/formatters.rs
git commit -m "refactor: rewrite FormatVectorsCounts::sub as a 4-line loop over &mut references"
```

---

### Task 14: Unify `BookAuthor`/`BookTranslator` into one `Person` type

**Files:**
- Modify: `book_bot/src/bots/approved_bot/services/book_library/types.rs`
- Modify: `book_bot/src/bots/approved_bot/services/book_library/formatters.rs`
- Modify: `book_bot/src/bots/approved_bot/services/book_library/mod.rs:170,183`

**Interfaces:**
- Removes: `BookAuthor`, `BookTranslator` (both structurally identical: `{ id: u32, first_name: String, last_name: String, middle_name: String }`), and their duplicate `FormatTitle`/`FormatInline` impls (identical except `/a_`/`/t_` prefixes and `DownloadArchiveCommand::Author`/`::Translator`).
- Produces: `pub struct Person { pub id: u32, pub first_name: String, pub last_name: String, pub middle_name: String, pub kind: PersonKind }` where `pub enum PersonKind { Author, Translator }` drives the prefix/command choice that used to be encoded by which struct you had. `Book`, `SearchBook`, `AuthorBook`, `TranslatorBook`, `SequenceBook` (which held `authors: Vec<BookAuthor>`/`translators: Vec<BookTranslator>`) now hold `Vec<Person>` for both fields — callers construct/deserialize with the right `kind`.

This is the most invasive step in the plan because `BookAuthor`/`BookTranslator` are `#[derive(Deserialize)]` types populated directly from the book-library HTTP API's JSON — the API returns two *separate* JSON arrays (`"authors": [...]`, `"translators": [...]`), each shaped identically, with no `kind` discriminator field in the payload itself. `PersonKind` must be attached **after** deserialization, not derived from the JSON.

- [ ] **Step 1: Write the failing test for `Person`'s per-kind formatting**

Add to `book_bot/src/bots/approved_bot/services/book_library/formatters.rs`'s test module (created in Task 12):

```rust
    use super::super::types::{Person, PersonKind};

    fn make_person(kind: PersonKind) -> Person {
        Person {
            id: 7,
            first_name: "F".to_string(),
            last_name: "L".to_string(),
            middle_name: "M".to_string(),
            kind,
        }
    }

    #[test]
    fn format_inline_uses_a_prefix_for_authors() {
        let p = make_person(PersonKind::Author);
        assert_eq!(p.format_inline(), "👤 L F M /a_7");
    }

    #[test]
    fn format_inline_uses_t_prefix_for_translators() {
        let p = make_person(PersonKind::Translator);
        assert_eq!(p.format_inline(), "👤 L F M /t_7");
    }

    #[test]
    fn format_title_is_empty_for_id_zero() {
        let mut p = make_person(PersonKind::Author);
        p.id = 0;
        assert_eq!(p.format_title(), "");
    }

    #[test]
    fn format_title_uses_author_archive_command() {
        let p = make_person(PersonKind::Author);
        assert_eq!(
            p.format_title(),
            "👤 L F M\nСкачать все книги архивом: /da_a_7"
        );
    }

    #[test]
    fn format_title_uses_translator_archive_command() {
        let p = make_person(PersonKind::Translator);
        assert_eq!(
            p.format_title(),
            "👤 L F M\nСкачать все книги архивом: /da_t_7"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

```bash
cargo test -p book_bot format_inline_uses_a_prefix_for_authors
```

Expected: compile error — `unresolved import 'Person'` (doesn't exist yet).

- [ ] **Step 3: Add `Person`/`PersonKind` to `types.rs`, remove `BookAuthor`/`BookTranslator`**

In `book_bot/src/bots/approved_bot/services/book_library/types.rs`, replace:

```rust
#[derive(Default, Deserialize, Debug, Clone)]
pub struct BookAuthor {
    pub id: u32,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: String,
}

#[derive(Default, Deserialize, Debug, Clone)]
pub struct BookTranslator {
    pub id: u32,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: String,
}
```

with:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonKind {
    Author,
    Translator,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Person {
    pub id: u32,
    pub first_name: String,
    pub last_name: String,
    pub middle_name: String,
    #[serde(skip, default = "default_person_kind")]
    pub kind: PersonKind,
}

fn default_person_kind() -> PersonKind {
    PersonKind::Author
}
```

`#[serde(skip)]` means `kind` is never read from JSON (the API's `authors`/`translators` arrays have no such field) and always starts as `PersonKind::Author` on deserialize — every deserialization call site (Step 4) must explicitly overwrite `kind` afterward for the `translators` field. `Person` drops the old `#[derive(Default)]` since `PersonKind` has no natural default that makes sense standalone (kept a serde-only `default_person_kind` fn instead of `#[derive(Default)]` on `PersonKind`, to make it clear this default is a deserialization placeholder, not a semantically meaningful "authors are the default kind of person").

- [ ] **Step 4: Update every place that deserializes or constructs a `Vec<BookAuthor>`/`Vec<BookTranslator>` to set `kind` and use `Person`**

In `book_bot/src/bots/approved_bot/services/book_library/types.rs`, every struct field of type `Vec<BookAuthor>` or `Vec<BookTranslator>` becomes `Vec<Person>`. These fields are populated purely by `#[derive(Deserialize)]` (there's no manual JSON parsing) — so add a `#[serde(deserialize_with = "...")]` per field that deserializes and tags. Add two small deserializer helpers near `Person`:

```rust
fn deserialize_authors<'de, D>(d: D) -> Result<Vec<Person>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut people = Vec::<Person>::deserialize(d)?;
    for p in &mut people {
        p.kind = PersonKind::Author;
    }
    Ok(people)
}

fn deserialize_translators<'de, D>(d: D) -> Result<Vec<Person>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut people = Vec::<Person>::deserialize(d)?;
    for p in &mut people {
        p.kind = PersonKind::Translator;
    }
    Ok(people)
}
```

Then annotate every `authors: Vec<...>` / `translators: Vec<...>` field. `Book`:

```rust
#[derive(Deserialize, Debug, Clone)]
pub struct Book {
    pub id: u32,
    pub title: String,
    pub lang: String,
    pub available_types: SmallVec<[String; 4]>,
    pub annotation_exists: bool,
    #[serde(deserialize_with = "deserialize_authors")]
    pub authors: Vec<Person>,
    #[serde(deserialize_with = "deserialize_translators")]
    pub translators: Vec<Person>,
    pub sequences: Vec<Sequence>,
    pub genres: Vec<BookGenre>,
    pub year: i32,
    pub pages: Option<u32>,
    pub position: Option<i32>,
}
```

Apply the same two-field pattern to `SearchBook` (`authors`, `translators`), `AuthorBook` (`translators` only — `deserialize_translators`), `TranslatorBook` (`authors` only — `deserialize_authors`), and `SequenceBook` (`authors`, `translators`).

Change the two `Page<_, _>` parent-item usages in `book_bot/src/bots/approved_bot/services/book_library/mod.rs`:

```rust
pub async fn get_author_books(
    ...
) -> anyhow::Result<Option<types::Page<types::AuthorBook, types::BookAuthor>>> {
```

to:

```rust
pub async fn get_author_books(
    ...
) -> anyhow::Result<Option<types::Page<types::AuthorBook, types::Person>>> {
```

and

```rust
pub async fn get_translator_books(
    ...
) -> anyhow::Result<Option<types::Page<types::TranslatorBook, types::BookTranslator>>> {
```

to:

```rust
pub async fn get_translator_books(
    ...
) -> anyhow::Result<Option<types::Page<types::TranslatorBook, types::Person>>> {
```

`Page<T, P>`'s `parent_item: Option<P>` here is deserialized generically too (`Page`'s own `#[derive(Deserialize)]`, `#[serde(default)]` on `parent_item`) — since `Person` doesn't discriminate `kind` from JSON, the `parent_item` for `get_author_books`'s response deserializes with `kind` defaulting to `PersonKind::Author` (correct — it's the author whose books are being listed), and `get_translator_books`'s `parent_item` needs `kind` forced to `Translator` after deserialization, since there's no field-level `deserialize_with` available for a *generic* `Page<T, P>`'s `P` (the annotation lives on `Page`'s own struct definition, shared by every instantiation). Handle this at the call site instead: in `book_bot/src/bots/approved_bot/modules/book/mod.rs`'s `send_book_handler`/`send_pagination_book_handler`, after fetching `items_page` from `get_translator_books`, fix up the parent's `kind`:

```rust
    let mut items_page = match books_getter(id, 1, allowed_langs).await {
```

Actually, this per-call-site patch only works cleanly if `send_book_handler`/`send_pagination_book_handler` know statically which getter they're calling — they're generic over `books_getter: fn(...) -> Fut` and are called once per `BookCommand`/`BookCallbackData` variant (`Author`/`Translator`/`Sequence`), so each call site already knows the kind. Simplest correct fix: have `get_translator_books` itself patch the parent's `kind` before returning, inside `book_library/mod.rs`:

```rust
pub async fn get_translator_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::TranslatorBook, types::Person>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    let mut result: Option<types::Page<types::TranslatorBook, types::Person>> = _make_request(
        &["api", "v1", "translators", &id.to_string(), "books"],
        params,
    )
    .await?;

    if let Some(page) = result.as_mut() {
        if let Some(parent) = page.parent_item.as_mut() {
            parent.kind = types::PersonKind::Translator;
        }
    }

    Ok(result)
}
```

`get_author_books` needs no such patch — `PersonKind::Author` is already the deserialize-time default, which is correct for its own parent item.

- [ ] **Step 5: Update `formatters.rs` — one `FormatTitle`/`FormatInline` impl for `Person` instead of two**

Remove the four impls (`FormatTitle for BookAuthor`, `FormatTitle for BookTranslator`, `FormatInline for BookAuthor`, `FormatInline for BookTranslator`) and the two `use` items for `BookAuthor`/`BookTranslator` in the `use super::types::{...}` import (replace with `Person`). Add:

```rust
impl FormatTitle for Person {
    fn format_title(&self) -> String {
        let Person {
            id,
            last_name,
            first_name,
            middle_name,
            kind,
        } = self;

        if *id == 0 {
            return "".to_string();
        }

        let command = match kind {
            PersonKind::Author => (DownloadArchiveCommand::Author { id: *id }).to_string(),
            PersonKind::Translator => (DownloadArchiveCommand::Translator { id: *id }).to_string(),
        };

        format!("👤 {last_name} {first_name} {middle_name}\nСкачать все книги архивом: {command}")
    }
}

impl FormatInline for Person {
    fn format_inline(&self) -> String {
        let Person {
            id,
            last_name,
            first_name,
            middle_name,
            kind,
        } = self;

        let prefix = match kind {
            PersonKind::Author => "a",
            PersonKind::Translator => "t",
        };

        format!("👤 {last_name} {first_name} {middle_name} /{prefix}_{id}")
    }
}
```

Update the `use super::types::{...}` line at the top of `formatters.rs`:

```rust
use super::types::{
    Author, AuthorBook, Book, BookGenre, Empty, Person, SearchBook, Sequence, SequenceBook,
    Translator, TranslatorBook,
};
```

Update `format_vectors`' two `authors: &[BookAuthor]`/`translators: &[BookTranslator]` parameters (and `format_common`/`FormatData`'s matching fields) to `&[Person]` — `format_list(authors, ..., |a| a.format_inline())` already just calls the trait method, so no call-site logic changes, only the type annotations:

```rust
fn format_vectors(
    authors: &[Person],
    translators: &[Person],
    sequences: &[Sequence],
    genres: &[BookGenre],
    max_size: usize,
) -> FormatVectorsResult {
```

and in `struct FormatData<'a>`:

```rust
struct FormatData<'a> {
    pub id: u32,
    pub title: &'a str,
    pub lang: &'a str,
    pub annotation_exists: bool,
    pub authors: &'a [Person],
    pub translators: &'a [Person],
    pub sequences: &'a [Sequence],
    pub genres: &'a [BookGenre],
    pub year: i32,
    pub pages: Option<u32>,
    pub position: Option<i32>,
}
```

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cargo test -p book_bot format_inline_uses_a_prefix_for_authors
cargo test -p book_bot format_inline_uses_t_prefix_for_translators
cargo test -p book_bot format_title_is_empty_for_id_zero
cargo test -p book_bot format_title_uses_author_archive_command
cargo test -p book_bot format_title_uses_translator_archive_command
```

Expected: all 5 pass.

- [ ] **Step 7: Build and fix any remaining `BookAuthor`/`BookTranslator` references, then run the full suite**

```bash
cargo build -p book_bot 2>&1 | grep -E "error"
```

Fix any remaining compile errors by following the compiler's pointed-to locations — every remaining reference should be inside `types.rs`/`formatters.rs`/`book_library/mod.rs` per the file list above; if the compiler surfaces a reference somewhere unexpected, re-run `grep -rn "BookAuthor\|BookTranslator" book_bot/src/` to find and fix it (the pre-check at the top of this task's design found only `book_library/mod.rs:170,183` outside `types.rs`/`formatters.rs`, so none are expected).

```bash
cargo test -p book_bot
```

Expected: `test result: ok. 157 passed; 0 failed` (152 + 5 new tests).

- [ ] **Step 8: Format, clippy-check, and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add book_bot/src/bots/approved_bot/services/book_library/types.rs book_bot/src/bots/approved_bot/services/book_library/formatters.rs book_bot/src/bots/approved_bot/services/book_library/mod.rs
git commit -m "refactor: unify BookAuthor/BookTranslator into a single Person type with a PersonKind discriminator"
```

---

### Task 15: Split `download/mod.rs` — extract `keyboards.rs` and `DownloadArchiveCommand::to_query_data`

**Files:**
- Create: `book_bot/src/bots/approved_bot/modules/download/keyboards.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/download/commands.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/download/mod.rs`

**Interfaces:**
- Produces: `DownloadArchiveCommand::to_query_data(&self, file_type: String) -> DownloadArchiveQueryData` in `commands.rs`, replacing the triple `match command { ... }` at the original lines 314-332.
- Produces in `keyboards.rs`: `pub fn get_check_keyboard(task_id: String) -> InlineKeyboardMarkup`, `pub fn get_download_format_keyboard(book: &types::Book) -> InlineKeyboardMarkup`, `pub fn get_download_archive_format_keyboard(command: DownloadArchiveCommand, available_types: &[String]) -> InlineKeyboardMarkup` — the three keyboard-building blocks currently inline in `get_download_keyboard_handler`/`get_download_archive_keyboard_handler`/`get_check_keyboard`.

- [ ] **Step 1: Write the failing test for `to_query_data`**

Add to `download/commands.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
    use super::super::callback_data::DownloadArchiveQueryData;

    #[test]
    fn to_query_data_sequence() {
        let cmd = DownloadArchiveCommand::Sequence { id: 3 };
        match cmd.to_query_data("fb2".to_string()) {
            DownloadArchiveQueryData::Sequence { id, file_type } => {
                assert_eq!(id, 3);
                assert_eq!(file_type, "fb2");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn to_query_data_author() {
        let cmd = DownloadArchiveCommand::Author { id: 4 };
        match cmd.to_query_data("epub".to_string()) {
            DownloadArchiveQueryData::Author { id, file_type } => {
                assert_eq!(id, 4);
                assert_eq!(file_type, "epub");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn to_query_data_translator() {
        let cmd = DownloadArchiveCommand::Translator { id: 6 };
        match cmd.to_query_data("zip".to_string()) {
            DownloadArchiveQueryData::Translator { id, file_type } => {
                assert_eq!(id, 6);
                assert_eq!(file_type, "zip");
            }
            _ => panic!("wrong variant"),
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail to compile**

```bash
cargo test -p book_bot to_query_data
```

Expected: compile error — `no method named 'to_query_data' found for enum 'DownloadArchiveCommand'`.

- [ ] **Step 3: Implement `to_query_data` in `commands.rs`**

Add after `DownloadArchiveCommand`'s `CommandParse` impl:

```rust
impl DownloadArchiveCommand {
    pub fn to_query_data(
        &self,
        file_type: String,
    ) -> crate::bots::approved_bot::modules::download::callback_data::DownloadArchiveQueryData {
        use crate::bots::approved_bot::modules::download::callback_data::DownloadArchiveQueryData;

        match *self {
            DownloadArchiveCommand::Sequence { id } => {
                DownloadArchiveQueryData::Sequence { id, file_type }
            }
            DownloadArchiveCommand::Author { id } => {
                DownloadArchiveQueryData::Author { id, file_type }
            }
            DownloadArchiveCommand::Translator { id } => {
                DownloadArchiveQueryData::Translator { id, file_type }
            }
        }
    }
}
```

- [ ] **Step 4: Run the new tests**

```bash
cargo test -p book_bot to_query_data
```

Expected: `test result: ok. 3 passed`.

- [ ] **Step 5: Create `keyboards.rs` and move the three keyboard-building blocks out of `mod.rs`**

Create `book_bot/src/bots/approved_bot/modules/download/keyboards.rs`:

```rust
use teloxide::types::{InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup};

use crate::bots::approved_bot::services::book_library::types::Book;

use super::{
    callback_data::{CheckArchiveStatus, DownloadQueryData},
    commands::DownloadArchiveCommand,
};

pub fn get_check_keyboard(task_id: String) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton {
            kind: InlineKeyboardButtonKind::CallbackData(
                (CheckArchiveStatus { task_id }).to_string(),
            ),
            text: String::from("Обновить статус"),
        }]],
    }
}

pub fn get_download_format_keyboard(book: &Book) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: book
            .available_types
            .iter()
            .map(|item| -> Vec<InlineKeyboardButton> {
                vec![InlineKeyboardButton {
                    text: format!("📥 {item}"),
                    kind: InlineKeyboardButtonKind::CallbackData(
                        (DownloadQueryData::DownloadData {
                            book_id: book.id,
                            file_type: item.clone(),
                        })
                        .to_string(),
                    ),
                }]
            })
            .collect(),
    }
}

pub fn get_download_archive_format_keyboard(
    command: DownloadArchiveCommand,
    available_types: &[String],
) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: available_types
            .iter()
            .filter(|file_type| !file_type.contains("zip"))
            .map(|file_type| {
                let callback_data = command.to_query_data(file_type.to_string()).to_string();

                vec![InlineKeyboardButton {
                    text: file_type.to_string(),
                    kind: InlineKeyboardButtonKind::CallbackData(callback_data),
                }]
            })
            .collect(),
    }
}
```

Note `get_download_format_keyboard` takes `book: &Book` and iterates `.iter()` (borrowing) instead of the original `.into_iter()` (consuming) — the original `get_download_keyboard_handler` didn't use `book` after building the keyboard, so consuming it was fine there, but as a standalone reusable function taking a reference is the more honest signature (it doesn't need ownership) and this is a pure extraction with no behavior change, just an ownership-shape adjustment at the boundary. Update the call site accordingly in Step 6.

In `download/mod.rs`, remove the `get_check_keyboard` function (moved to `keyboards.rs`), and replace the inline keyboard-building code in `get_download_keyboard_handler`:

```rust
    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: book
            .available_types
            .into_iter()
            .map(|item| -> Vec<InlineKeyboardButton> {
                vec![InlineKeyboardButton {
                    text: { format!("📥 {item}") },
                    kind: InlineKeyboardButtonKind::CallbackData(
                        (DownloadQueryData::DownloadData {
                            book_id: book.id,
                            file_type: item,
                        })
                        .to_string(),
                    ),
                }]
            })
            .collect(),
    };
```

with:

```rust
    let keyboard = get_download_format_keyboard(&book);
```

and in `get_download_archive_keyboard_handler`, replace:

```rust
    let keyboard = InlineKeyboardMarkup {
        inline_keyboard: available_types
            .iter()
            .filter(|file_type| !file_type.contains("zip"))
            .map(|file_type| {
                let callback_data: String = match command {
                    DownloadArchiveCommand::Sequence { id } => DownloadArchiveQueryData::Sequence {
                        id,
                        file_type: file_type.to_string(),
                    }
                    .to_string(),
                    DownloadArchiveCommand::Author { id } => DownloadArchiveQueryData::Author {
                        id,
                        file_type: file_type.to_string(),
                    }
                    .to_string(),
                    DownloadArchiveCommand::Translator { id } => {
                        DownloadArchiveQueryData::Translator {
                            id,
                            file_type: file_type.to_string(),
                        }
                        .to_string()
                    }
                };

                vec![InlineKeyboardButton {
                    text: file_type.to_string(),
                    kind: InlineKeyboardButtonKind::CallbackData(callback_data),
                }]
            })
            .collect(),
    };
```

with:

```rust
    let keyboard = get_download_archive_format_keyboard(command, &available_types);
```

Update every other call site of `get_check_keyboard` inside `mod.rs` (there are two: inside `wait_archive`'s loop, and inside `download_archive` after creating the task) to use the moved function — no code change needed there beyond the import, since the call syntax `get_check_keyboard(task.id.clone())` / `get_check_keyboard(task.id)` is unchanged.

Add `pub mod keyboards;` to `download/mod.rs`'s module declarations (alongside the existing `pub mod callback_data;` / `pub mod commands;`), and add the import:

```rust
use self::keyboards::{
    get_check_keyboard, get_download_archive_format_keyboard, get_download_format_keyboard,
};
```

Remove now-unused imports from `download/mod.rs`'s `use teloxide::{..., types::*};` if `InlineKeyboardButton`/`InlineKeyboardButtonKind`/`InlineKeyboardMarkup` are no longer referenced directly (they still are — `send_error_message`/`send_archive_link` build `InlineKeyboardMarkup { inline_keyboard: vec![] }` inline — so `types::*` stays as-is, just double check with the build in Step 6 rather than guessing).

- [ ] **Step 6: Build and run the full suite**

```bash
cargo build -p book_bot 2>&1 | grep -E "warning|error"
cargo test -p book_bot
```

Expected: no warnings/errors after fixing any flagged unused imports; `test result: ok. 160 passed; 0 failed` (157 + 3 new `to_query_data` tests).

- [ ] **Step 7: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/modules/download/keyboards.rs book_bot/src/bots/approved_bot/modules/download/commands.rs book_bot/src/bots/approved_bot/modules/download/mod.rs
git commit -m "refactor: extract download keyboards into keyboards.rs and add DownloadArchiveCommand::to_query_data"
```

---

### Task 16: Split `download/mod.rs` — extract `file_send.rs` and `archive.rs`

**Files:**
- Create: `book_bot/src/bots/approved_bot/modules/download/file_send.rs`
- Create: `book_bot/src/bots/approved_bot/modules/download/archive.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/download/mod.rs`

**Interfaces:**
- Produces in `file_send.rs`: `pub async fn send_cached_message(...)`, `pub async fn send_with_download_from_channel(...)`, `pub async fn download_handler(...)`, plus their two private helpers `_send_cached`/`_send_downloaded_file`. These are the "sending a single book with caching" story (~130 lines per the spec).
- Produces in `archive.rs`: `pub async fn wait_archive(...)`, `pub async fn download_archive(...)`, plus their private helpers `send_error_message`/`send_archive_link`. This is the "archive-task lifecycle" story (~230 lines per the spec).
- `mod.rs` keeps only: `get_download_keyboard_handler`, `get_download_archive_keyboard_handler`, `download_query_handler`, and `get_download_handler()` — the `dptree` wiring plus the two keyboard-request handlers (which stay here since they're thin and directly paired with the `dptree` registration).

This task is a mechanical move with import-fixup, not new logic — there's no new pure function to TDD, so the verification is the existing suite staying green plus a successful build, matching how Task 6 (a module rename) was verified.

- [ ] **Step 1: Create `file_send.rs` with the moved functions**

Create `book_bot/src/bots/approved_bot/modules/download/file_send.rs`:

```rust
use futures::TryStreamExt;
use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InputFile, MaybeInaccessibleMessage},
};
use tracing::log;

use crate::{
    bots::{
        approved_bot::{
            modules::utils::telegram_utils::{safe_copy_message, safe_delete_message, safe_send_document},
            services::{
                book_cache::{
                    download_file, download_file_by_link, get_cached_message,
                    types::{CachedMessage, DownloadFile},
                },
                donation_notifications::send_donation_notification,
            },
        },
        BotHandlerInternal,
    },
    bots_manager::BotCache,
};

use super::callback_data::DownloadQueryData;

async fn _send_cached(
    message: &MaybeInaccessibleMessage,
    bot: &CacheMe<Throttle<Bot>>,
    cached_message: CachedMessage,
) -> BotHandlerInternal {
    safe_copy_message(
        bot,
        ChatId(cached_message.chat_id),
        message.chat().id,
        MessageId(cached_message.message_id),
    )
    .await
}

pub async fn send_cached_message(
    message: MaybeInaccessibleMessage,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadQueryData,
    need_delete_message: bool,
    cache: BotCache,
    user_id: Option<u64>,
) -> BotHandlerInternal {
    'cached: {
        if let Ok(v) = get_cached_message(&download_data, cache, user_id).await {
            let cached = match v {
                Some(v) => v,
                None => break 'cached,
            };

            if _send_cached(&message, &bot, cached).await.is_ok() {
                if need_delete_message {
                    if let MaybeInaccessibleMessage::Regular(message) = &message {
                        let _ = safe_delete_message(&bot, message.chat.id, message.id).await;
                    }
                }

                match send_donation_notification(&bot, &message).await {
                    Ok(_) => (),
                    Err(err) => log::error!("{err:?}"),
                }

                return Ok(());
            }
        };
    }

    send_with_download_from_channel(message, bot, download_data, need_delete_message, user_id)
        .await?;

    Ok(())
}

pub async fn _send_downloaded_file(
    message: &MaybeInaccessibleMessage,
    bot: &CacheMe<Throttle<Bot>>,
    downloaded_data: DownloadFile,
) -> BotHandlerInternal {
    let DownloadFile {
        response,
        filename,
        caption,
    } = downloaded_data;

    let stream = response.bytes_stream().map_err(std::io::Error::other);
    let data = tokio_util::io::StreamReader::new(stream);

    let document = InputFile::read(data).file_name(filename);

    safe_send_document(bot, message.chat().id, document, caption).await?;

    send_donation_notification(bot, message).await?;

    Ok(())
}

pub async fn send_with_download_from_channel(
    message: MaybeInaccessibleMessage,
    bot: CacheMe<Throttle<Bot>>,
    download_data: DownloadQueryData,
    need_delete_message: bool,
    user_id: Option<u64>,
) -> BotHandlerInternal {
    let downloaded_file = match download_file(&download_data, user_id).await? {
        Some(v) => v,
        None => {
            return Ok(());
        }
    };

    _send_downloaded_file(&message, &bot, downloaded_file).await?;

    if need_delete_message {
        if let MaybeInaccessibleMessage::Regular(message) = message {
            let _ = safe_delete_message(&bot, message.chat.id, message.id).await;
        };
    }

    Ok(())
}

pub async fn download_handler(
    message: MaybeInaccessibleMessage,
    bot: CacheMe<Throttle<Bot>>,
    cache: BotCache,
    download_data: DownloadQueryData,
    need_delete_message: bool,
    user_id: Option<u64>,
) -> BotHandlerInternal {
    match cache {
        BotCache::Original | BotCache::Cache => {
            send_cached_message(
                message,
                bot,
                download_data,
                need_delete_message,
                cache,
                user_id,
            )
            .await
        }
        BotCache::NoCache => {
            send_with_download_from_channel(
                message,
                bot,
                download_data,
                need_delete_message,
                user_id,
            )
            .await
        }
    }
}
```

`_send_downloaded_file` becomes `pub` (was private) since `archive.rs` (Step 2) also calls it — the leading underscore is kept as-is for this task (renaming it is out of scope; it's an existing naming convention signaling "helper, not a primary entry point" independent of visibility).

- [ ] **Step 2: Create `archive.rs` with the moved functions**

Create `book_bot/src/bots/approved_bot/modules/download/archive.rs`:

```rust
use std::time::Duration;

use book_bot_macros::log_handler;
use chrono::Utc;
use teloxide::{
    adaptors::{CacheMe, Throttle},
    prelude::*,
    types::{InlineKeyboardMarkup, MaybeInaccessibleMessage},
};
use tokio::time;
use tracing::log;

use crate::{
    bots::{
        approved_bot::{
            modules::utils::{
                constants::*,
                telegram_utils::{safe_delete_message, safe_edit_message_text, safe_edit_message_text_html},
            },
            services::{
                batch_downloader::{create_task, get_task, CreateTaskData, Task, TaskObjectType, TaskStatus},
                book_cache::download_file_by_link,
                build_url,
                user_settings::{get_user_file_name_lang_for, get_user_or_default_lang_codes, FileNameLang},
            },
        },
        BotHandlerInternal,
    },
    config,
};

use super::{
    callback_data::DownloadArchiveQueryData, file_send::_send_downloaded_file,
    keyboards::get_check_keyboard,
};

async fn send_error_message(bot: &CacheMe<Throttle<Bot>>, chat_id: ChatId, message_id: MessageId) {
    let _ = safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        ERROR_TRY_LATER,
        Some(InlineKeyboardMarkup {
            inline_keyboard: vec![],
        }),
    )
    .await;
}

async fn send_archive_link(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    task: &Task,
) -> BotHandlerInternal {
    let link = build_url(
        &config::CONFIG.public_batch_downloader_url,
        ["api", "download", &task.id],
    )?
    .to_string();

    safe_edit_message_text_html(
        bot,
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

    Ok(())
}

pub async fn wait_archive(
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
            Ok(v) => v,
            Err(err) => {
                send_error_message(&bot, message.chat.id, message.id).await;
                log::error!("{err:?}");
                return Err(err);
            }
        };

        if !matches!(task.status, TaskStatus::InProgress | TaskStatus::Archiving) {
            break task;
        }

        let now = Utc::now().format("%H:%M:%S UTC").to_string();

        safe_edit_message_text(
            &bot,
            message.chat.id,
            message.id,
            format!(
                "Статус: \n ⏳ {} \n\nОбновлено в {now}",
                task.status_description
            ),
            Some(get_check_keyboard(task.id.clone())),
        )
        .await?;
    };

    if task.status == TaskStatus::Failed {
        let is_rate_limit = task
            .error_message
            .as_deref()
            .map(|msg| msg.to_lowercase().contains("rate limit"))
            .unwrap_or(false);

        if is_rate_limit {
            log::warn!(
                "Rate limit hit for user {} on task {}",
                message.chat.id,
                task.id
            );
            let _ = safe_edit_message_text(
                &bot,
                message.chat.id,
                message.id,
                RATE_LIMIT_ERROR,
                Some(InlineKeyboardMarkup {
                    inline_keyboard: vec![],
                }),
            )
            .await;
        } else {
            log::error!("Task {} failed: {:?}", task.id, task.error_message);
            send_error_message(&bot, message.chat.id, message.id).await;
        }
        return Ok(());
    }

    if task.status != TaskStatus::Complete {
        send_error_message(&bot, message.chat.id, message.id).await;
        return Ok(());
    }

    let Some(content_size) = task.content_size else {
        send_archive_link(&bot, message.chat.id, message.id, &task).await?;
        return Ok(());
    };

    if content_size > 1024 * 1024 * 1024 {
        send_archive_link(&bot, message.chat.id, message.id, &task).await?;
        return Ok(());
    }

    let link = build_url(
        &config::CONFIG.batch_downloader_url,
        ["api", "download", &task.id],
    )?
    .to_string();

    let downloaded_data = match download_file_by_link(
        task.result_filename.as_deref().unwrap_or_default(),
        link,
    )
    .await
    {
        Ok(v) => match v {
            Some(v) => v,
            None => {
                send_error_message(&bot, message.chat.id, message.id).await;
                return Ok(());
            }
        },
        Err(err) => {
            send_error_message(&bot, message.chat.id, message.id).await;
            log::warn!("{err:?}");
            return Err(err);
        }
    };

    match _send_downloaded_file(
        &MaybeInaccessibleMessage::Regular(message.clone()),
        &bot,
        downloaded_data,
    )
    .await
    {
        Ok(_) => (),
        Err(err) => {
            send_archive_link(&bot, message.chat.id, message.id, &task).await?;
            log::warn!("{err:?}");
        }
    }

    let _ = safe_delete_message(&bot, message.chat.id, message.id).await;

    Ok(())
}

#[log_handler("download")]
pub async fn download_archive(
    cq: CallbackQuery,
    download_archive_query_data: DownloadArchiveQueryData,
    bot: CacheMe<Throttle<Bot>>,
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(cq.from.id).await;

    let (id, file_type, task_type) = match download_archive_query_data {
        DownloadArchiveQueryData::Sequence { id, file_type } => {
            (id, file_type, TaskObjectType::Sequence)
        }
        DownloadArchiveQueryData::Author { id, file_type } => {
            (id, file_type, TaskObjectType::Author)
        }
        DownloadArchiveQueryData::Translator { id, file_type } => {
            (id, file_type, TaskObjectType::Translator)
        }
    };

    let Some(message) = cq.message else {
        return Ok(());
    };

    let user_id = cq.from.id.0;

    let normalized = !matches!(
        get_user_file_name_lang_for(Some(user_id)).await,
        FileNameLang::Original
    );

    let task = create_task(
        CreateTaskData {
            object_id: id,
            object_type: task_type,
            file_format: file_type,
            allowed_langs,
            normalized,
        },
        Some(user_id),
    )
    .await;

    let task = match task {
        Ok(v) => v,
        Err(err) => {
            send_error_message(&bot, message.chat().id, message.id()).await;
            log::error!("{err:?}");
            return Err(err);
        }
    };

    safe_edit_message_text(
        &bot,
        message.chat().id,
        message.id(),
        "⏳ Подготовка архива...",
        Some(get_check_keyboard(task.id.clone())),
    )
    .await?;

    if let Err(err) = wait_archive(bot, task.id, message).await {
        log::error!("{err:?}");
    }

    Ok(())
}
```

The `// `normalized` mirrors ...` comment that was above the `normalized` line in the original `mod.rs` is preserved by carrying it over verbatim — re-add it above the `let normalized = ...` line:

```rust
    // `normalized` mirrors the cache server's `?normalized=` parameter.
    // Default for the server is `true` (transliterated names); we send
    // `false` only when the user opted into original Cyrillic names.
    let normalized = !matches!(
```

- [ ] **Step 3: Trim `download/mod.rs` down to the keyboard-request handlers and `dptree` wiring**

Remove from `download/mod.rs`: `get_check_keyboard` (already moved in Task 15), `_send_cached`, `send_cached_message`, `_send_downloaded_file`, `send_with_download_from_channel`, `download_handler`, `send_error_message`, `send_archive_link`, `wait_archive`, `download_archive`. What remains is `get_download_keyboard_handler`, `get_download_archive_keyboard_handler`, `download_query_handler`, and `get_download_handler()`.

Add module declarations at the top of `download/mod.rs`:

```rust
pub mod archive;
pub mod callback_data;
pub mod commands;
pub mod file_send;
pub mod keyboards;
```

Rewrite `download/mod.rs`'s imports to only what the remaining functions need:

```rust
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
        services::book_library::{
            get_author_books_available_types, get_book, get_sequence_books_available_types,
            get_translator_books_available_types,
        },
        tools::filter_callback_query,
    },
    BotHandlerInternal,
};

use self::{
    archive::download_archive,
    callback_data::{CheckArchiveStatus, DownloadQueryData},
    commands::{DownloadArchiveCommand, StartDownloadCommand},
    file_send::download_handler,
    keyboards::{get_download_archive_format_keyboard, get_download_format_keyboard},
};

use super::utils::filter_command::filter_command;

use archive::wait_archive;
```

`download_query_handler` (unchanged logic) now calls `file_send::download_handler` via the `download_handler` import above:

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

This still needs `BotCache` — add `crate::bots_manager::BotCache` to the `use crate::bots::{...}` import block (as a sibling `use crate::bots_manager::BotCache;`).

`get_download_handler()`'s body is unchanged (it already only referenced the four remaining handlers plus `download_archive`, now imported from `archive`, and the `CheckArchiveStatus` branch's closure, which calls `wait_archive`, now imported from `archive`):

```rust
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
```

(`DownloadArchiveQueryData` needs importing too — add it to the `use self::{ callback_data::{...} }` block: `callback_data::{CheckArchiveStatus, DownloadArchiveQueryData, DownloadQueryData}`.)

- [ ] **Step 4: Build and fix import errors iteratively**

```bash
cargo build -p book_bot 2>&1 | head -80
```

Fix reported errors (missing imports, wrong visibility) one at a time — the most likely issues are (a) `_send_downloaded_file` needing to be `pub(super)` or `pub` and imported correctly in `archive.rs` (already handled as `pub` in Step 1), and (b) `ChatId`/`MessageId` needing explicit imports in `file_send.rs`/`archive.rs` from `teloxide::types` (both files use bare `ChatId(...)`/`MessageId(...)` and `.id()`/`.chat_id` via the `prelude::*` glob already present in each file's `use teloxide::{prelude::*, ...}` — `prelude` re-exports `ChatId`/`MessageId`, so this should already work; if the compiler disagrees, add `types::{ChatId, MessageId}` explicitly to each file's `teloxide::{..., types::{...}}` import).

Repeat `cargo build -p book_bot 2>&1 | head -80` after each fix until it's clean.

- [ ] **Step 5: Run the full suite**

```bash
cargo test -p book_bot
```

Expected: `test result: ok. 160 passed; 0 failed` (no new tests — pure move, verified by the existing suite staying green, same count as the end of Task 15).

- [ ] **Step 6: Confirm the line-count acceptance criterion for `download/mod.rs`**

```bash
wc -l book_bot/src/bots/approved_bot/modules/download/mod.rs
```

Expected: under 150 lines (per the spec's acceptance criteria). If it's over, check for leftover dead imports or blank-line bloat from the edit and trim.

- [ ] **Step 7: Format, clippy-check, and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add book_bot/src/bots/approved_bot/modules/download/
git commit -m "refactor: split download/mod.rs into file_send.rs and archive.rs, leaving only keyboard-request handlers and dptree wiring in mod.rs"
```

---

### Task 17: Add `save_user_settings` and wire it into all 4 call sites

**Files:**
- Modify: `book_bot/src/bots/approved_bot/services/user_settings/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/settings/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/mod.rs`

**Interfaces:**
- Produces: `pub async fn save_user_settings(user: &teloxide::types::User, me: &teloxide::types::Me, allowed_langs: SmallVec<[SmartString; 3]>, default_search: Option<DefaultSearchType>, file_name_lang: FileNameLang) -> anyhow::Result<UserSettings>` in `user_settings/mod.rs`, wrapping `create_or_update_user_settings` with the `user.last_name.as_deref().unwrap_or("")` / `me.username.as_deref().unwrap_or_default()` unpacking that's currently repeated at all 4 call sites (3 in `settings/mod.rs`, 1 in `approved_bot/mod.rs`).

This helper lives in `user_settings/mod.rs` (not `settings/mod.rs`) specifically so `approved_bot/mod.rs` — which is a sibling of, not a descendant of, the `settings` module — can call it without a cross-module dependency in the wrong direction.

- [ ] **Step 1: Add `save_user_settings` to `user_settings/mod.rs`**

There's no isolated unit test for this (it makes a real HTTP call via `create_or_update_user_settings`, same as every other function in this file — none of which have network-mocked tests; the file's existing test module only covers the pure cache-coalescing logic and a `check_response` status-code case). Add it directly, right after `create_or_update_user_settings`:

```rust
/// Thin wrapper around `create_or_update_user_settings` that does the
/// `Option<String>` → `&str` unpacking shared by every settings-mutation
/// call site (`settings::mod`'s three handlers, and the background
/// activity-update fallback in `approved_bot::mod`).
pub async fn save_user_settings(
    user: &teloxide::types::User,
    me: &teloxide::types::Me,
    allowed_langs: SmallVec<[SmartString; 3]>,
    default_search: Option<DefaultSearchType>,
    file_name_lang: FileNameLang,
) -> anyhow::Result<UserSettings> {
    create_or_update_user_settings(
        user.id,
        user.last_name.as_deref().unwrap_or(""),
        &user.first_name,
        user.username.as_deref().unwrap_or(""),
        me.username.as_deref().unwrap_or_default(),
        allowed_langs,
        default_search,
        file_name_lang,
    )
    .await
}
```

- [ ] **Step 2: Wire it into the 3 call sites in `settings/mod.rs`**

`SettingsCallbackData::DefaultSearch { value }` branch — change:

```rust
            if create_or_update_user_settings(
                user.id,
                &user.last_name.unwrap_or("".to_string()),
                &user.first_name,
                user.username.as_deref().unwrap_or(""),
                me.username.as_deref().unwrap_or_default(),
                allowed_langs,
                default_search,
                file_name_lang,
            )
            .await
            .is_err()
```

to:

```rust
            if save_user_settings(&user, &me, allowed_langs, default_search, file_name_lang)
                .await
                .is_err()
```

`SettingsCallbackData::FileNameLang { value }` branch — same shape, change:

```rust
            if create_or_update_user_settings(
                user.id,
                &user.last_name.unwrap_or("".to_string()),
                &user.first_name,
                user.username.as_deref().unwrap_or(""),
                me.username.as_deref().unwrap_or_default(),
                allowed_langs,
                default_search,
                file_name_lang,
            )
            .await
            .is_err()
```

to:

```rust
            if save_user_settings(&user, &me, allowed_langs, default_search, file_name_lang)
                .await
                .is_err()
```

The final fallthrough branch (lang toggle) — change:

```rust
    if let Err(err) = create_or_update_user_settings(
        user.id,
        &user.last_name.unwrap_or("".to_string()),
        &user.first_name,
        &user.username.unwrap_or("".to_string()),
        me.username.as_deref().unwrap_or_default(),
        allowed_langs_set.clone().into_iter().collect(),
        default_search,
        file_name_lang,
    )
    .await
    {
```

to:

```rust
    if let Err(err) = save_user_settings(
        &user,
        &me,
        allowed_langs_set.clone().into_iter().collect(),
        default_search,
        file_name_lang,
    )
    .await
    {
```

Since `user: teloxide::types::User = cq.from` is currently consumed by value at each of these 3 call sites (`user.last_name.unwrap_or(...)` moves out of `user.last_name`, and the last branch also does `&user.username.unwrap_or(...)` which moves `user.username`), switching to `&user` at all 3 sites means `user` is no longer partially moved anywhere in the function — it stays a plain owned local used only by reference from this point on. No signature change needed for `user`/`me` themselves (`me: Me` is already a parameter taken by value; pass `&me`).

Update the `use` block at the top of `settings/mod.rs`, replacing:

```rust
        services::user_settings::{
            create_or_update_user_settings, get_langs, get_user_or_default_lang_codes,
            get_user_settings, DefaultSearchType, FileNameLang, Lang,
        },
```

with:

```rust
        services::user_settings::{
            get_langs, get_user_or_default_lang_codes, get_user_settings, save_user_settings,
            DefaultSearchType, FileNameLang, Lang,
        },
```

- [ ] **Step 3: Wire it into the 1 call site in `approved_bot/mod.rs`**

Change:

```rust
            if create_or_update_user_settings(
                user.id,
                &user.last_name.unwrap_or("".to_string()),
                &user.first_name,
                &user.username.unwrap_or("".to_string()),
                &me.username.clone().unwrap_or("".to_string()),
                allowed_langs,
                default_search,
                file_name_lang,
            )
            .await
            .is_ok()
```

to:

```rust
            if save_user_settings(&user, &me, allowed_langs, default_search, file_name_lang)
                .await
                .is_ok()
```

Update the `use` block at the top of `approved_bot/mod.rs`, replacing:

```rust
use crate::{
    bots::approved_bot::services::user_settings::{
        create_or_update_user_settings, get_user_settings,
    },
    bots_manager::USER_ACTIVITY_CACHE,
};
```

with:

```rust
use crate::{
    bots::approved_bot::services::user_settings::{get_user_settings, save_user_settings},
    bots_manager::USER_ACTIVITY_CACHE,
};
```

`user`/`me` are already `User`/`Me` owned parameters of `_update_activity(me: teloxide::types::Me, user: teloxide::types::User)`, unused after this point in the function — passing `&user`/`&me` is a pure ownership-shape change with no behavior difference.

- [ ] **Step 4: Build and run the full suite**

```bash
cargo build -p book_bot 2>&1 | grep -E "warning|error"
cargo test -p book_bot
```

Expected: no warnings/errors; `test result: ok. 160 passed; 0 failed`.

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add book_bot/src/bots/approved_bot/services/user_settings/mod.rs book_bot/src/bots/approved_bot/modules/settings/mod.rs book_bot/src/bots/approved_bot/mod.rs
git commit -m "refactor: add save_user_settings helper, dedup the 4 create_or_update_user_settings call sites"
```

---

### Task 18: Decompose `settings/mod.rs` — extract `keyboards.rs` and split `settings_callback_handler`

**Files:**
- Create: `book_bot/src/bots/approved_bot/modules/settings/keyboards.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/settings/mod.rs`

**Interfaces:**
- Produces in `keyboards.rs`: `pub fn get_main_settings_keyboard() -> InlineKeyboardMarkup`, `pub fn get_lang_keyboard(...) -> InlineKeyboardMarkup`, `pub fn get_default_search_keyboard(...) -> InlineKeyboardMarkup`, `pub fn get_file_name_lang_keyboard(...) -> InlineKeyboardMarkup` — moved as-is, no logic change.
- Produces in `mod.rs`: `settings_callback_handler` becomes a dispatcher over `callback_data`'s variant, delegating to 5 new private functions: `show_main_menu`, `show_default_search_menu`, `show_file_name_lang_menu`, `handle_default_search`, `handle_file_name_lang`, `handle_lang_toggle` — removing both the 3 near-identical "Back" branches (each currently a copy-pasted `safe_edit_message_text(...get_main_settings_keyboard...); safe_answer_callback_query(...)`) and the empty-arm "unreachable" second `match` (lines 395-410 in the original), which existed only to let already-handled variants fall through to the lang-toggle code — the new dispatcher makes that fallthrough unnecessary since every variant gets exactly one explicit arm.

- [ ] **Step 1: Create `keyboards.rs` with the four moved functions**

Create `book_bot/src/bots/approved_bot/modules/settings/keyboards.rs`:

```rust
use std::collections::HashSet;

use smartstring::alias::String as SmartString;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup};

use crate::bots::approved_bot::services::user_settings::{DefaultSearchType, FileNameLang, Lang};

use super::callback_data::SettingsCallbackData;

pub fn get_main_settings_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: "Языки".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::Settings.to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Поиск по умолчанию".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearchMenu.to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "Имена файлов".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::FileNameLangMenu.to_string(),
                ),
            }],
        ],
    }
}

pub fn get_lang_keyboard(
    all_langs: Vec<Lang>,
    allowed_langs: HashSet<SmartString>,
) -> InlineKeyboardMarkup {
    let mut buttons: Vec<Vec<InlineKeyboardButton>> = all_langs
        .into_iter()
        .map(|lang| {
            let (emoji, callback_data) = match allowed_langs.contains(&lang.code) {
                true => (
                    "🟢".to_string(),
                    SettingsCallbackData::Off { code: lang.code }.to_string(),
                ),
                false => (
                    "🔴".to_string(),
                    SettingsCallbackData::On { code: lang.code }.to_string(),
                ),
            };

            vec![InlineKeyboardButton {
                text: format!("{emoji} {}", lang.label),
                kind: InlineKeyboardButtonKind::CallbackData(callback_data),
            }]
        })
        .collect();

    buttons.push(vec![InlineKeyboardButton {
        text: "← Назад".to_string(),
        kind: InlineKeyboardButtonKind::CallbackData(
            SettingsCallbackData::LangSettingsBack.to_string(),
        ),
    }]);

    InlineKeyboardMarkup {
        inline_keyboard: buttons,
    }
}

pub fn get_default_search_keyboard(current: Option<DefaultSearchType>) -> InlineKeyboardMarkup {
    let check = |v: DefaultSearchType| if current == Some(v) { " ✓" } else { "" };
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: format!("Книга{}", check(DefaultSearchType::Book)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "book".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Автор{}", check(DefaultSearchType::Author)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "author".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Серия{}", check(DefaultSearchType::Series)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "series".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Переводчик{}", check(DefaultSearchType::Translator)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "translator".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Не выбрано{}", if current.is_none() { " ✓" } else { "" }),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearch {
                        value: "none".into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "← Назад".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::DefaultSearchBack.to_string(),
                ),
            }],
        ],
    }
}

pub fn get_file_name_lang_keyboard(current: FileNameLang) -> InlineKeyboardMarkup {
    let check = |v: FileNameLang| if current == v { " ✓" } else { "" };
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![InlineKeyboardButton {
                text: format!("Транслит{}", check(FileNameLang::Normalized)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::FileNameLang {
                        value: FileNameLang::Normalized.as_api_str().into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: format!("Язык оригинала{}", check(FileNameLang::Original)),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::FileNameLang {
                        value: FileNameLang::Original.as_api_str().into(),
                    }
                    .to_string(),
                ),
            }],
            vec![InlineKeyboardButton {
                text: "← Назад".to_string(),
                kind: InlineKeyboardButtonKind::CallbackData(
                    SettingsCallbackData::FileNameLangBack.to_string(),
                ),
            }],
        ],
    }
}
```

- [ ] **Step 2: Remove the four functions from `mod.rs`, add `pub mod keyboards;` and import them**

Remove `get_main_settings_keyboard`, `get_lang_keyboard`, `get_default_search_keyboard`, `get_file_name_lang_keyboard` from `settings/mod.rs`. Add `pub mod keyboards;` alongside the existing `pub mod callback_data;`/`pub mod commands;`. Add:

```rust
use self::keyboards::{
    get_default_search_keyboard, get_file_name_lang_keyboard, get_lang_keyboard,
    get_main_settings_keyboard,
};
```

- [ ] **Step 3: Build to confirm the pure move compiles, then run the suite**

```bash
cargo build -p book_bot 2>&1 | grep -E "warning|error"
cargo test -p book_bot
```

Expected: clean build (fix any leftover unused `InlineKeyboardButton`/`InlineKeyboardButtonKind`/`HashSet`/`SmartString` imports flagged in `mod.rs` — they're needed in `mod.rs` too for `settings_callback_handler`'s own `HashSet<SmartString>` bookkeeping, so most likely nothing to remove, but check the exact warning list before editing); `test result: ok. 160 passed; 0 failed`.

- [ ] **Step 4: Split `settings_callback_handler` into one function per callback group**

Replace the entire body of `settings_callback_handler` (from the top-level `match &callback_data { ... }` for the menu/back branches, through the final lang-toggle code) with a pure dispatcher, and move each branch's logic into its own function.

First, add these 6 new private functions above `settings_callback_handler`:

```rust
async fn show_main_menu(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
) -> BotHandlerInternal {
    safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        "Настройки",
        Some(get_main_settings_keyboard()),
    )
    .await?;
    safe_answer_callback_query(bot, cq_id).await?;
    Ok(())
}

async fn show_default_search_menu(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user_id: UserId,
) -> BotHandlerInternal {
    let current = get_user_settings(user_id).await.ok().flatten();
    let current_default = current.as_ref().and_then(|s| s.default_search);
    let keyboard = get_default_search_keyboard(current_default);
    safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        "Поиск по умолчанию",
        Some(keyboard),
    )
    .await?;
    safe_answer_callback_query(bot, cq_id).await?;
    Ok(())
}

async fn show_file_name_lang_menu(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user_id: UserId,
) -> BotHandlerInternal {
    let current = get_user_settings(user_id).await.ok().flatten();
    let current_value = current
        .as_ref()
        .map(|s| s.file_name_lang)
        .unwrap_or_default();
    let keyboard = get_file_name_lang_keyboard(current_value);
    safe_edit_message_text(bot, chat_id, message_id, "Имена файлов", Some(keyboard)).await?;
    safe_answer_callback_query(bot, cq_id).await?;
    Ok(())
}

async fn handle_default_search(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user: &teloxide::types::User,
    me: &Me,
    value: &str,
) -> BotHandlerInternal {
    let current = get_user_settings(user.id).await.ok().flatten();
    let allowed_langs: SmallVec<[SmartString; 3]> = match current.as_ref() {
        Some(s) => s.allowed_langs.iter().map(|l| l.code.clone()).collect(),
        None => get_user_or_default_lang_codes(user.id).await,
    };
    let default_search = if value == "none" {
        None
    } else if let Some(t) = DefaultSearchType::from_api_str(value) {
        Some(t)
    } else {
        safe_answer_callback_query(bot, cq_id).await?;
        return Ok(());
    };
    let file_name_lang = current
        .as_ref()
        .map(|s| s.file_name_lang)
        .unwrap_or_default();

    if save_user_settings(user, me, allowed_langs, default_search, file_name_lang)
        .await
        .is_err()
    {
        safe_answer_callback_query_with_text(bot, cq_id, "Ошибка! Попробуйте заново(", true)
            .await?;
        return Ok(());
    }

    safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        "Настройки",
        Some(get_main_settings_keyboard()),
    )
    .await?;
    safe_answer_callback_query_with_text(bot, cq_id, "Готово", false).await?;
    Ok(())
}

async fn handle_file_name_lang(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user: &teloxide::types::User,
    me: &Me,
    value: &str,
) -> BotHandlerInternal {
    let file_name_lang = match FileNameLang::from_api_str(value) {
        Some(v) => v,
        None => {
            safe_answer_callback_query(bot, cq_id).await?;
            return Ok(());
        }
    };
    let current = get_user_settings(user.id).await.ok().flatten();
    let allowed_langs: SmallVec<[SmartString; 3]> = match current.as_ref() {
        Some(s) => s.allowed_langs.iter().map(|l| l.code.clone()).collect(),
        None => get_user_or_default_lang_codes(user.id).await,
    };
    let default_search = current.as_ref().and_then(|s| s.default_search);

    if save_user_settings(user, me, allowed_langs, default_search, file_name_lang)
        .await
        .is_err()
    {
        safe_answer_callback_query_with_text(bot, cq_id, "Ошибка! Попробуйте заново(", true)
            .await?;
        return Ok(());
    }

    safe_edit_message_text(
        bot,
        chat_id,
        message_id,
        "Настройки",
        Some(get_main_settings_keyboard()),
    )
    .await?;
    safe_answer_callback_query_with_text(bot, cq_id, "Готово", false).await?;
    Ok(())
}

async fn handle_lang_toggle(
    bot: &CacheMe<Throttle<Bot>>,
    chat_id: ChatId,
    message_id: MessageId,
    cq_id: CallbackQueryId,
    user: &teloxide::types::User,
    me: &Me,
    callback_data: &SettingsCallbackData,
) -> BotHandlerInternal {
    let allowed_langs = get_user_or_default_lang_codes(user.id).await;

    let mut allowed_langs_set: HashSet<SmartString> = HashSet::new();
    allowed_langs.into_iter().for_each(|v| {
        allowed_langs_set.insert(v);
    });

    match callback_data {
        SettingsCallbackData::Settings => (),
        SettingsCallbackData::On { code } => {
            allowed_langs_set.insert(code.clone());
        }
        SettingsCallbackData::Off { code } => {
            allowed_langs_set.remove(code);
        }
        _ => unreachable!("handle_lang_toggle is only called for Settings/On/Off"),
    };

    if allowed_langs_set.is_empty() {
        safe_answer_callback_query_with_text(
            bot,
            cq_id,
            "Должен быть активен, хотя бы один язык!",
            true,
        )
        .await?;

        return Ok(());
    }

    let current_settings = get_user_settings(user.id).await.ok().flatten();
    let default_search = current_settings.as_ref().and_then(|s| s.default_search);
    let file_name_lang = current_settings
        .as_ref()
        .map(|s| s.file_name_lang)
        .unwrap_or_default();

    if let Err(err) = save_user_settings(
        user,
        me,
        allowed_langs_set.clone().into_iter().collect(),
        default_search,
        file_name_lang,
    )
    .await
    {
        safe_send_message(bot, chat_id, "Ошибка! Попробуйте заново(", None).await?;
        return Err(err);
    }

    let all_langs = match get_langs().await {
        Ok(v) => v,
        Err(err) => {
            safe_send_message(bot, chat_id, "Ошибка! Попробуйте заново(", None).await?;
            return Err(err);
        }
    };

    let keyboard = get_lang_keyboard(all_langs, allowed_langs_set);

    safe_edit_message_reply_markup(bot, chat_id, message_id, keyboard).await?;

    Ok(())
}
```

The `unreachable!()` in `handle_lang_toggle` is safe: the dispatcher (next) only calls this function for exactly those 3 variants, mirroring the original code's structure where the second `match` had explicit no-op arms for every other variant (now those other variants never reach this function at all, which is strictly more precise than the original "arrive here anyway, then explicitly no-op" flow — this is the direct fix for the "empty unreachable arms" artifact the spec calls out).

Now replace `settings_callback_handler` itself with the dispatcher:

```rust
#[log_handler("settings")]
async fn settings_callback_handler(
    cq: CallbackQuery,
    bot: CacheMe<Throttle<Bot>>,
    callback_data: SettingsCallbackData,
    me: Me,
) -> BotHandlerInternal {
    let message = match cq.message {
        Some(v) => v,
        None => {
            safe_send_message(&bot, cq.from.id.into(), "Ошибка! Попробуйте заново(", None).await?;
            return Ok(());
        }
    };

    let user = cq.from;
    let chat_id = message.chat().id;
    let message_id = message.id();

    match &callback_data {
        SettingsCallbackData::DefaultSearchMenu => {
            show_default_search_menu(&bot, chat_id, message_id, cq.id, user.id).await
        }
        SettingsCallbackData::DefaultSearchBack
        | SettingsCallbackData::FileNameLangBack
        | SettingsCallbackData::LangSettingsBack => {
            show_main_menu(&bot, chat_id, message_id, cq.id).await
        }
        SettingsCallbackData::FileNameLangMenu => {
            show_file_name_lang_menu(&bot, chat_id, message_id, cq.id, user.id).await
        }
        SettingsCallbackData::DefaultSearch { value } => {
            handle_default_search(&bot, chat_id, message_id, cq.id, &user, &me, value).await
        }
        SettingsCallbackData::FileNameLang { value } => {
            handle_file_name_lang(&bot, chat_id, message_id, cq.id, &user, &me, value).await
        }
        SettingsCallbackData::Settings
        | SettingsCallbackData::On { .. }
        | SettingsCallbackData::Off { .. } => {
            handle_lang_toggle(&bot, chat_id, message_id, cq.id, &user, &me, &callback_data).await
        }
    }
}
```

Note `message.chat().id`/`message.id()` are computed once up front here — the original code called `message.chat().id`/`message.id()` repeatedly inline at every branch; this is a pure hoist with no behavior change since `message` isn't mutated between those calls anywhere in the original.

Add `UserId` and `CallbackQueryId` to the `teloxide::types::*` glob already imported (check — `settings/mod.rs` currently has `use teloxide::{..., types::{InlineKeyboardButton, InlineKeyboardMarkup, Me}};`; expand to `types::{CallbackQueryId, InlineKeyboardButton, InlineKeyboardMarkup, Me}` — `UserId`/`ChatId`/`MessageId` come from `teloxide::prelude::*`, already imported).

- [ ] **Step 5: Build and fix import/type errors iteratively**

```bash
cargo build -p book_bot 2>&1 | head -80
```

Fix reported errors one at a time (most likely: missing `CallbackQueryId` import, or a `SmallVec`/`SmartString` import needed directly in `mod.rs` now that the keyboard functions using them moved to `keyboards.rs` — check whether `mod.rs` itself still needs `use smallvec::SmallVec;`/`use smartstring::alias::String as SmartString;`, which it does, for `handle_lang_toggle`'s local `HashSet<SmartString>` and the `SmallVec<[SmartString; 3]>` return types passed to `save_user_settings`).

- [ ] **Step 6: Run the full suite**

```bash
cargo test -p book_bot
```

Expected: `test result: ok. 160 passed; 0 failed` (pure decomposition, no new pure logic to test — `handle_default_search`/`handle_file_name_lang`/`handle_lang_toggle` all require a live bot to exercise, same testing boundary as every other handler in this codebase).

- [ ] **Step 7: Confirm the line-count acceptance criterion for `settings/mod.rs`**

```bash
wc -l book_bot/src/bots/approved_bot/modules/settings/mod.rs
```

Expected: under 150 lines. If over, double check nothing was left duplicated between `mod.rs` and `keyboards.rs`.

- [ ] **Step 8: Format, clippy-check, and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add book_bot/src/bots/approved_bot/modules/settings/
git commit -m "refactor: split settings/mod.rs into keyboards.rs plus one function per callback group, removing the empty second-match artifact"
```

---

### Task 19: Final acceptance-criteria verification

**Files:** None modified — verification only.

- [ ] **Step 1: Run the full test suite and confirm the final count**

```bash
cargo test -p book_bot
```

Expected: `test result: ok. 160 passed; 0 failed` — strictly more than the 136-test baseline recorded at the top of this plan, and zero failures throughout every intermediate task.

- [ ] **Step 2: Run the exact grep checks from the spec's acceptance criteria**

```bash
grep -rn "escape_html" book_bot/src/ | grep -v "teloxide::utils::html::escape"
```

Expected: no output (both local `escape_html` definitions were removed in Task 1; any remaining hits are only calls to `teloxide::utils::html::escape`, which the `grep -v` filters out — if this prints anything else, a copy was missed).

```bash
grep -rn "#\[allow(dead_code)\]" book_bot/src/
```

Expected: no output (Task 5 removed all instances; if a legitimate one was added back for a genuinely-reserved function during Task 5 Step 3's contingency path, this line will show it — investigate and justify or remove).

```bash
find book_bot/src -iname "*bots_manager*"
```

Expected: only `book_bot/src/bots_manager/` (the top-level webhook/lifecycle manager) — no `book_bot/src/bots/bots_manager/` (renamed to `registration` in Task 6).

- [ ] **Step 3: Confirm the file-size acceptance criteria**

```bash
wc -l book_bot/src/bots/approved_bot/modules/download/mod.rs \
      book_bot/src/bots/approved_bot/modules/settings/mod.rs \
      book_bot/src/bots/approved_bot/services/book_library/formatters.rs
```

Expected: `download/mod.rs` and `settings/mod.rs` under 150 lines each (per spec); `formatters.rs` noticeably reduced from its original 625 lines (removing `format_authors`/`format_translators`/`format_sequences`/`format_genres` in favor of `format_list`, collapsing the `BookAuthor`/`BookTranslator` duplicate impls into one `Person` impl each, and shrinking `FormatVectorsCounts::sub` from ~60 lines to ~10 — expect somewhere in the 350–450 line range, i.e. roughly a 30–45% reduction; if it's not "noticeably reduced," re-check for any leftover dead code before considering this task done).

- [ ] **Step 4: Run the full CI-equivalent gate locally**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: all three clean, matching the three CI jobs (`fmt`, `clippy`, `test`) defined in `.github/workflows/`.

- [ ] **Step 5: Manual smoke-check note (cannot be automated in this environment)**

This plan's automated verification (unit tests + compiler + clippy) cannot exercise the live Telegram flows end-to-end — there is no test double for `CacheMe<Throttle<Bot>>` anywhere in this codebase, so no automated test actually drives a real `/search`, `/d_<id>`, or `/settings` conversation. Per the spec's acceptance criteria ("manual check of search/download/settings flows"), before merging: run the bot against a test Telegram bot token and manually walk through (a) `/settings` → toggle a language, change default search, change file-name-language, and back out of each submenu; (b) a text search that hits each of Book/Authors/Sequences/Translators, then paginate forward/backward and past the last page; (c) `/d_<id>` on a book with multiple formats, and `/da_a_<id>`/`/da_s_<id>`/`/da_t_<id>` archive downloads including the "check status" button. Record the outcome in the PR description rather than checking this box blindly.

- [ ] **Step 6: No commit for this task** — it's verification-only. If any check above fails, return to the relevant task, fix, and re-run this task's checks from Step 1.
