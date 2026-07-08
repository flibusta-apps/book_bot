# Panic Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate all reachable panics on externally-controlled data (Telegram messages, external service responses, callback data) so that no single bad input can crash the entire multi-bot process.

**Architecture:** Each fix is a targeted, self-contained change to the relevant file. No new abstractions are introduced beyond what the spec explicitly prescribes. Tests are added inline (`#[cfg(test)]`) in the same file as the fixed code, following the existing pattern in the codebase.

**Tech Stack:** Rust, teloxide 0.17, anyhow, reqwest, serde_json, base64 0.22, parking_lot (to be added for Task 7d if chosen).

## Global Constraints

- `panic = "abort"` is set in the workspace `Cargo.toml` `[profile.release]` — any panic in release kills all bots.
- No new dependencies unless explicitly noted; follow existing code style.
- Tests use `#[cfg(test)] mod tests { ... }` in-file, not a separate test file.
- Run `cargo test -p book_bot` to verify; run `cargo clippy -p book_bot` to check for warnings.
- `cargo build -p book_bot` must succeed after every task.

---

### Task 1: Fix triple `unwrap()` on cache-server response headers (spec §2.1)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/services/book_cache/mod.rs:179-193`

**Interfaces:**
- No interface changes; the surrounding function already returns `anyhow::Result<Option<DownloadFile>>`.

- [ ] **Step 1: Write the failing test**

Add at the bottom of `book_bot/src/bots/approved_bot/services/book_cache/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::decode_b64_header;
    use reqwest::header::HeaderMap;

    #[test]
    fn missing_header_returns_err() {
        let headers = HeaderMap::new();
        assert!(decode_b64_header(&headers, "x-filename-b64").is_err());
    }

    #[test]
    fn invalid_base64_returns_err() {
        let mut headers = HeaderMap::new();
        headers.insert("x-test", "not!!base64".parse().unwrap());
        assert!(decode_b64_header(&headers, "x-test").is_err());
    }

    #[test]
    fn non_utf8_returns_err() {
        use base64::{engine::general_purpose, Engine as _};
        let mut headers = HeaderMap::new();
        let encoded = general_purpose::STANDARD.encode([0xFF, 0xFE]);
        headers.insert("x-test", encoded.parse().unwrap());
        assert!(decode_b64_header(&headers, "x-test").is_err());
    }

    #[test]
    fn valid_header_decoded() {
        use base64::{engine::general_purpose, Engine as _};
        let mut headers = HeaderMap::new();
        let encoded = general_purpose::STANDARD.encode("hello.epub");
        headers.insert("x-test", encoded.parse().unwrap());
        assert_eq!(decode_b64_header(&headers, "x-test").unwrap(), "hello.epub");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```
cargo test -p book_bot decode_b64_header
```

Expected: `error[E0425]: cannot find function 'decode_b64_header'`

- [ ] **Step 3: Add the helper function and fix the call sites**

In `book_bot/src/bots/approved_bot/services/book_cache/mod.rs`, add this function (place it just before the existing function that uses the headers, around line 165):

```rust
fn decode_b64_header(headers: &reqwest::header::HeaderMap, name: &str) -> anyhow::Result<String> {
    use anyhow::Context as _;
    use base64::{engine::general_purpose, Engine as _};

    let raw = headers
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("missing response header: {name}"))?;
    let decoded_bytes = general_purpose::STANDARD
        .decode(raw)
        .with_context(|| format!("invalid base64 in header {name}"))?;
    std::str::from_utf8(&decoded_bytes)
        .with_context(|| format!("non-UTF8 content in header {name}"))
        .map(|s| s.to_string())
}
```

Then replace lines 179–193 (the two `unwrap`-chain blocks) with:

```rust
    let filename = decode_b64_header(headers, "x-filename-b64")?;
    let caption = decode_b64_header(headers, "x-caption-b64")?;
```

- [ ] **Step 4: Run tests to verify they pass**

```
cargo test -p book_bot decode_b64_header
```

Expected: all 4 tests PASS.

- [ ] **Step 5: Verify it builds**

```
cargo build -p book_bot
```

Expected: no errors, no warnings about dead code.

- [ ] **Step 6: Commit**

```bash
git add book_bot/src/bots/approved_bot/services/book_cache/mod.rs
git commit -m "fix(book_cache): replace triple unwrap on headers with decode_b64_header helper"
```

---

### Task 2: Fix page=0 underflow in annotation and book callback data (spec §2.2)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/annotations/callback_data.rs:27`
- Modify: `book_bot/src/bots/approved_bot/modules/book/callback_data.rs:28`
- Modify: `book_bot/src/bots/approved_bot/modules/annotations/mod.rs:161,171`

The `search/callback_data.rs` already has the fix (`std::cmp::max(1, page)`) — mirror that pattern.

**Interfaces:**
- `AnnotationCallbackData::Book { id, page }` and `::Author { id, page }` — `page` is now guaranteed `>= 1` after `FromStr`.
- `BookCallbackData::Author { id, page }`, `::Translator`, `::Sequence` — same guarantee.

- [ ] **Step 1: Write the failing tests**

Add at the bottom of `book_bot/src/bots/approved_bot/modules/annotations/callback_data.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::AnnotationCallbackData;
    use std::str::FromStr;

    #[test]
    fn page_zero_normalized_to_one_book() {
        let cd = AnnotationCallbackData::from_str("b_an_5_0").unwrap();
        match cd {
            AnnotationCallbackData::Book { page, .. } => assert_eq!(page, 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn page_zero_normalized_to_one_author() {
        let cd = AnnotationCallbackData::from_str("a_an_5_0").unwrap();
        match cd {
            AnnotationCallbackData::Author { page, .. } => assert_eq!(page, 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn normal_page_preserved() {
        let cd = AnnotationCallbackData::from_str("b_an_42_3").unwrap();
        match cd {
            AnnotationCallbackData::Book { id, page } => {
                assert_eq!(id, 42);
                assert_eq!(page, 3);
            }
            _ => panic!("wrong variant"),
        }
    }
}
```

Add at the bottom of `book_bot/src/bots/approved_bot/modules/book/callback_data.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::BookCallbackData;
    use std::str::FromStr;

    #[test]
    fn page_zero_normalized_to_one() {
        let cd = BookCallbackData::from_str("ba_5_0").unwrap();
        match cd {
            BookCallbackData::Author { page, .. } => assert_eq!(page, 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn normal_page_preserved() {
        let cd = BookCallbackData::from_str("bs_7_4").unwrap();
        match cd {
            BookCallbackData::Sequence { id, page } => {
                assert_eq!(id, 7);
                assert_eq!(page, 4);
            }
            _ => panic!("wrong variant"),
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cargo test -p book_bot page_zero_normalized
```

Expected: FAIL — `assertion failed: page == 1` (page is 0, not 1).

- [ ] **Step 3: Fix AnnotationCallbackData::from_str**

In `book_bot/src/bots/approved_bot/modules/annotations/callback_data.rs`, change the `page` parsing line from:

```rust
        let page: u32 = caps["page"].parse().map_err(|_| CallbackQueryParseError)?;
```

to:

```rust
        let page: u32 = std::cmp::max(1, caps["page"].parse::<u32>().map_err(|_| CallbackQueryParseError)?);
```

- [ ] **Step 4: Fix BookCallbackData::from_str**

In `book_bot/src/bots/approved_bot/modules/book/callback_data.rs`, change the `page` parsing line from:

```rust
        let page: u32 = caps["page"].parse().map_err(|_| CallbackQueryParseError)?;
```

to:

```rust
        let page: u32 = std::cmp::max(1, caps["page"].parse::<u32>().map_err(|_| CallbackQueryParseError)?);
```

- [ ] **Step 5: Fix the `.get(page_index - 1).unwrap()` in annotations/mod.rs**

In `book_bot/src/bots/approved_bot/modules/annotations/mod.rs`, the block at lines ~161–171 currently reads:

```rust
    let request_page: usize = page.try_into().unwrap();

    let annotation_text = annotation.get_text();
    let chunked_text = split_text_to_chunks(annotation_text, 512);

    let page_index = if request_page <= chunked_text.len() {
        request_page
    } else {
        chunked_text.len()
    };
    let new_text = chunked_text.get(page_index - 1).unwrap();
```

Replace with:

```rust
    let request_page: usize = page.try_into().unwrap_or(1);

    let annotation_text = annotation.get_text();
    let chunked_text = split_text_to_chunks(annotation_text, 512);

    let page_index = if request_page <= chunked_text.len() {
        request_page
    } else {
        chunked_text.len()
    };

    let new_text = match chunked_text.get(page_index.saturating_sub(1)) {
        Some(t) => t,
        None => return Ok(()),
    };
```

Also fix the `.unwrap()` on line ~111 (`send_annotation_handler`):

```rust
    let current_text = chunked_text.first().unwrap();
```

Replace with:

```rust
    let current_text = match chunked_text.first() {
        Some(t) => t,
        None => return Ok(()),
    };
```

- [ ] **Step 6: Run all tests**

```
cargo test -p book_bot page_zero_normalized
```

Expected: all 5 tests PASS.

- [ ] **Step 7: Build**

```
cargo build -p book_bot
```

Expected: success.

- [ ] **Step 8: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/annotations/callback_data.rs \
        book_bot/src/bots/approved_bot/modules/book/callback_data.rs \
        book_bot/src/bots/approved_bot/modules/annotations/mod.rs
git commit -m "fix(annotations): normalize page=0 to 1 in callback_data; replace unsafe .get().unwrap() in pagination"
```

---

### Task 3: Fix `unwrap()` on `content_size` from batch_downloader (spec §2.3)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/download/mod.rs:477`

**Interfaces:**
- `task.content_size: Option<u64>` — treat `None` as "large file" (send archive link instead of trying to size-check).

- [ ] **Step 1: Locate the exact line**

`book_bot/src/bots/approved_bot/modules/download/mod.rs:477`:

```rust
    let content_size = task.content_size.unwrap();
```

- [ ] **Step 2: Replace with `let Some(...) else`**

Replace:

```rust
    let content_size = task.content_size.unwrap();
```

with:

```rust
    let Some(content_size) = task.content_size else {
        send_archive_link(&bot, message.chat.id, message.id, &task).await?;
        return Ok(());
    };
```

(When `content_size` is absent the file size is unknown — treating it as large and sending the link is the safest fallback.)

- [ ] **Step 3: Build**

```
cargo build -p book_bot
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/download/mod.rs
git commit -m "fix(download): handle None content_size without panic; fall back to archive link"
```

---

### Task 4: Fix `get_me().await.unwrap()` and `me.username.unwrap()` (spec §2.4)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/mod.rs:79,85`
- Modify: `book_bot/src/bots/approved_bot/modules/utils/filter_command.rs:14`
- Modify: `book_bot/src/bots/approved_bot/modules/settings/mod.rs:310,357,436`

**Interfaces:**
- `filter_command.rs`: `me.user.username` is `Option<String>`; bots always have usernames in practice, but we must not panic on `None`.
- `settings/mod.rs`: `me` is injected by dptree as `Me`; `me.username` is `Option<String>`.

- [ ] **Step 1: Fix `mod.rs` — `bot.get_me().await.unwrap()` in inspect_async**

In `book_bot/src/bots/approved_bot/mod.rs`, the `update_user_activity_handler` function (lines ~76-89) currently reads:

```rust
fn update_user_activity_handler() -> BotHandler {
    dptree::entry()
        .branch(Update::filter_callback_query().inspect_async(
            |cq: CallbackQuery, bot: CacheMe<Throttle<Bot>>| async move {
                _update_activity(bot.get_me().await.unwrap(), cq.from).await;
            },
        ))
        .branch(Update::filter_message().inspect_async(
            |message: Message, bot: CacheMe<Throttle<Bot>>| async move {
                if let Some(user) = message.from {
                    _update_activity(bot.get_me().await.unwrap(), user).await;
                }
            },
        ))
}
```

Replace with:

```rust
fn update_user_activity_handler() -> BotHandler {
    dptree::entry()
        .branch(Update::filter_callback_query().inspect_async(
            |cq: CallbackQuery, bot: CacheMe<Throttle<Bot>>| async move {
                if let Ok(me) = bot.get_me().await {
                    _update_activity(me, cq.from).await;
                }
            },
        ))
        .branch(Update::filter_message().inspect_async(
            |message: Message, bot: CacheMe<Throttle<Bot>>| async move {
                if let Some(user) = message.from {
                    if let Ok(me) = bot.get_me().await {
                        _update_activity(me, user).await;
                    }
                }
            },
        ))
}
```

- [ ] **Step 2: Fix `filter_command.rs` — `me.user.username.expect(...)`**

In `book_bot/src/bots/approved_bot/modules/utils/filter_command.rs`, line 14:

```rust
        let bot_name = me.user.username.expect("Bots must have a username");
```

Replace with:

```rust
        let bot_name = me.user.username.unwrap_or_default();
```

(An empty bot name means the command parse will simply fail to match, which is safe.)

- [ ] **Step 3: Fix `settings/mod.rs` — three `me.username.clone().unwrap()` calls**

In `book_bot/src/bots/approved_bot/modules/settings/mod.rs`, there are three identical occurrences (lines ~310, ~357, ~436):

```rust
                &me.username.clone().unwrap(),
```

Replace all three with:

```rust
                me.username.as_deref().unwrap_or_default(),
```

(The function signature for `create_or_update_user_settings` takes `&str` for the bot_username parameter; `as_deref()` converts `Option<String>` → `Option<&str>`, then `unwrap_or_default()` gives `""` on `None`.)

- [ ] **Step 4: Build**

```
cargo build -p book_bot
```

Expected: success.

- [ ] **Step 5: Commit**

```bash
git add book_bot/src/bots/approved_bot/mod.rs \
        book_bot/src/bots/approved_bot/modules/utils/filter_command.rs \
        book_bot/src/bots/approved_bot/modules/settings/mod.rs
git commit -m "fix(activity,settings): replace get_me().unwrap() and username.unwrap() with safe fallbacks"
```

---

### Task 5: Fix division-by-zero and usize underflow in page formatting (spec §2.5)

**Files:**
- Modify: `book_bot/src/bots/approved_bot/services/book_library/types.rs:113-144`
- Modify: `book_bot/src/bots/approved_bot/services/book_library/formatters.rs:507`

**Interfaces:**
- `Page::format_items` — currently `fn format_items(&self, max_size: usize) -> String`; behavior change: returns `""` immediately when `self.items` is empty.
- `Page::format` — has an unguarded `max_size - title.len() - footer.len()` subtraction at line 108; guard with `saturating_sub`.
- `format_common` in `formatters.rs` — `max_size - required_data_len` subtraction at line 507; guard with `saturating_sub`.
- `format_items` free-symbols line 160 — `new_item_size - new_formated_result.current_size` — guard with `saturating_sub`.

- [ ] **Step 1: Write failing tests**

Add to `book_bot/src/bots/approved_bot/services/book_library/types.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::approved_bot::services::book_library::formatters::FormatResult;

    // Minimal concrete types for testing Page::format_items
    #[derive(Clone, Debug)]
    struct FakeItem;

    impl crate::bots::approved_bot::services::book_library::formatters::Format for FakeItem {
        fn format(&self, max_size: usize) -> FormatResult {
            let s = "x".to_string();
            FormatResult { current_size: s.len(), max_size, result: s }
        }
    }

    #[derive(Clone, Debug)]
    struct FakeParent;

    impl crate::bots::approved_bot::services::book_library::formatters::FormatTitle for FakeParent {
        fn format_title(&self) -> String {
            "parent".to_string()
        }
    }

    #[test]
    fn format_items_empty_does_not_panic() {
        let page: Page<FakeItem, FakeParent> = Page {
            items: vec![],
            pages: 1,
            parent_item: None,
        };
        let result = page.format_items(100);
        assert_eq!(result, "");
    }

    #[test]
    fn format_items_small_max_size_does_not_panic() {
        let page: Page<FakeItem, FakeParent> = Page {
            items: vec![FakeItem, FakeItem],
            pages: 1,
            parent_item: None,
        };
        // max_size smaller than the separators — previously could underflow
        let result = page.format_items(2);
        // should not panic; result may be truncated or empty
        let _ = result;
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```
cargo test -p book_bot format_items_empty
```

Expected: FAIL with thread panic `attempt to divide by zero`.

- [ ] **Step 3: Fix `format_items` in types.rs**

In `book_bot/src/bots/approved_bot/services/book_library/types.rs`, the `format_items` function (starting at line ~113) currently starts with:

```rust
    fn format_items(&self, max_size: usize) -> String {
        let separator = "\n\n\n";
        let separator_len: usize = separator.len();

        let items_count: usize = self.items.len();
        let item_size: usize = (max_size - separator_len * items_count) / items_count;
```

Replace with:

```rust
    fn format_items(&self, max_size: usize) -> String {
        if self.items.is_empty() {
            return String::new();
        }

        let separator = "\n\n\n";
        let separator_len: usize = separator.len();

        let items_count: usize = self.items.len();
        let item_size: usize = max_size
            .saturating_sub(separator_len * items_count)
            / items_count;
```

- [ ] **Step 4: Fix the free-symbols underflow in `format_items`**

In the same function, the calculation at line ~143:

```rust
        let mut free_symbols: usize = format_result
            .iter()
            .filter(|item| item.current_size == item.max_size)
            .map(|item| item_size - item.current_size)
            .sum();
```

Replace with:

```rust
        let mut free_symbols: usize = format_result
            .iter()
            .filter(|item| item.current_size == item.max_size)
            .map(|item| item_size.saturating_sub(item.current_size))
            .sum();
```

And line ~160:

```rust
                    free_symbols = new_item_size - new_formated_result.current_size;
```

Replace with:

```rust
                    free_symbols = new_item_size.saturating_sub(new_formated_result.current_size);
```

- [ ] **Step 5: Fix the `max_size - title.len() - footer.len()` subtraction in `Page::format`**

In `book_bot/src/bots/approved_bot/services/book_library/types.rs`, at line ~108:

```rust
        let formated_items = self.format_items(max_size - title.len() - footer.len());
```

Replace with:

```rust
        let formated_items = self.format_items(max_size.saturating_sub(title.len()).saturating_sub(footer.len()));
```

- [ ] **Step 6: Fix the `max_size - required_data_len` underflow in `formatters.rs`**

In `book_bot/src/bots/approved_bot/services/book_library/formatters.rs`, at line ~507:

```rust
        max_size - required_data_len,
```

(This is the `format_vectors` call inside `format_common`.) Replace with:

```rust
        max_size.saturating_sub(required_data_len),
```

- [ ] **Step 7: Run tests**

```
cargo test -p book_bot format_items
```

Expected: both tests PASS.

- [ ] **Step 8: Build**

```
cargo build -p book_bot
```

Expected: success.

- [ ] **Step 9: Commit**

```bash
git add book_bot/src/bots/approved_bot/services/book_library/types.rs \
        book_bot/src/bots/approved_bot/services/book_library/formatters.rs
git commit -m "fix(formatters): guard against division-by-zero on empty items and usize underflow in format_items"
```

---

### Task 6: Fix panics on `from`/token in bots_manager module (spec §2.6)

**Files:**
- Modify: `book_bot/src/bots/bots_manager/mod.rs:19,50`
- Modify: `book_bot/src/bots/bots_manager/register.rs:63`

**Interfaces:**
- `message_handler` — `message.from` is `Option<User>`; `None` for anonymous group admins and channel forwards.
- `get_manager_handler` — the `dptree::filter` currently calls `get_token(...).is_some()`, then `message_handler` calls `get_token` again and unwraps.
- `register::register(user_id, message_text)` — the second `get_token` call in `register.rs:63` duplicates the work already done by the filter; it must not panic when the filter has already guaranteed a token exists.

Fix approach: return early from `message_handler` when `message.from` is `None`; in `register.rs`, replace the `.unwrap()` with an early return (the filter guarantees a token, but we still must not panic).

- [ ] **Step 1: Fix `message_handler` in `bots_manager/mod.rs`**

In `book_bot/src/bots/bots_manager/mod.rs`, at line 19:

```rust
    let from_user = message.clone().from.unwrap();
```

Replace with:

```rust
    let from_user = match message.from.clone() {
        Some(user) => user,
        None => return Ok(()),
    };
```

- [ ] **Step 2: Fix the second `get_token` call in `mod.rs` — the `dptree::filter` closure**

At line 50:

```rust
            .chain(dptree::filter(|message: Message| {
                get_token(message.text().unwrap()).is_some()
            }))
```

`message.text()` can be `None` for non-text messages — although `Message::filter_text()` is chained before this, it is safer to use `unwrap_or_default()`:

Replace with:

```rust
            .chain(dptree::filter(|message: Message| {
                get_token(message.text().unwrap_or_default()).is_some()
            }))
```

- [ ] **Step 3: Fix the `.unwrap()` in `register.rs`**

In `book_bot/src/bots/bots_manager/register.rs`, line 63:

```rust
    let token = super::utils::get_token(message_text).unwrap();
```

Replace with:

```rust
    let token = match super::utils::get_token(message_text) {
        Some(t) => t,
        None => return RegisterStatus::WrongToken,
    };
```

- [ ] **Step 4: Build**

```
cargo build -p book_bot
```

Expected: success.

- [ ] **Step 5: Commit**

```bash
git add book_bot/src/bots/bots_manager/mod.rs \
        book_bot/src/bots/bots_manager/register.rs
git commit -m "fix(bots_manager): handle None message.from for anonymous admins; remove unwrap on get_token"
```

---

### Task 7: Minor cases (spec §2.7)

This task covers five small fixes. They are bundled into one commit since each change is a single line.

**Files:**
- Modify: `book_bot/src/config.rs:44`
- Modify: `book_bot/src/main.rs:20-27`
- Modify: `book_bot/src/bots_manager/closable_sender.rs:23,27`
- Modify: `book_bot/src/bots/approved_bot/modules/random/mod.rs:107`
- Modify: `book_bot/src/bots/approved_bot/services/batch_downloader.rs:64`

- [ ] **Step 1: Fix `WEBHOOK_PORT` parse panic in config.rs**

In `book_bot/src/config.rs`, line 44:

```rust
            webhook_port: get_env("WEBHOOK_PORT").parse().unwrap(),
```

Replace with:

```rust
            webhook_port: get_env("WEBHOOK_PORT")
                .parse()
                .unwrap_or_else(|_| panic!("Cannot parse WEBHOOK_PORT")),
```

- [ ] **Step 2: Make Sentry DSN optional in main.rs and config.rs**

In `book_bot/src/config.rs`, change the struct field type from `sentry_dsn: String` to `sentry_dsn: Option<String>` and the load line from:

```rust
            sentry_dsn: get_env("SENTRY_DSN"),
```

to:

```rust
            sentry_dsn: std::env::var("SENTRY_DSN").ok(),
```

In `book_bot/src/main.rs`, replace:

```rust
    let options = ClientOptions {
        dsn: Some(Dsn::from_str(&config::CONFIG.sentry_dsn).unwrap()),
        default_integrations: false,
        ..Default::default()
    }
    .add_integration(DebugImagesIntegration::new());

    let _guard = sentry::init(options);
```

with:

```rust
    let _guard = if let Some(dsn_str) = &config::CONFIG.sentry_dsn {
        let dsn = Dsn::from_str(dsn_str)
            .unwrap_or_else(|_| panic!("Cannot parse SENTRY_DSN"));
        let options = ClientOptions {
            dsn: Some(dsn),
            default_integrations: false,
            ..Default::default()
        }
        .add_integration(DebugImagesIntegration::new());
        sentry::init(options)
    } else {
        sentry::init(())
    };
```

Also remove the `use sentry::types::Dsn;` and `use std::str::FromStr;` imports from `main.rs` if they are now unused — or keep them; the compiler will warn. The `use std::str::FromStr;` is still needed for `Dsn::from_str`. Keep it.

- [ ] **Step 3: Fix `RwLock` unwrap in `closable_sender.rs`**

In `book_bot/src/bots_manager/closable_sender.rs`, line 23:

```rust
        self.origin.read().unwrap().clone()
```

Replace with:

```rust
        self.origin.read().unwrap_or_else(|e| e.into_inner()).clone()
```

Line 27:

```rust
        self.origin.write().unwrap().take();
```

Replace with:

```rust
        self.origin.write().unwrap_or_else(|e| e.into_inner()).take();
```

- [ ] **Step 4: Fix `cq.data.unwrap()` in `random/mod.rs`**

In `book_bot/src/bots/approved_bot/modules/random/mod.rs`, line 107:

```rust
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(cq.data.unwrap()),
```

`cq.data` is `Option<String>` and is `Some` whenever dptree has matched a callback query (it had to parse the data to reach this handler), but the type does not guarantee that. Replace with `unwrap_or_default()`:

```rust
                kind: teloxide::types::InlineKeyboardButtonKind::CallbackData(
                    cq.data.clone().unwrap_or_default(),
                ),
```

`clone()` is needed because `cq` is used again after this expression (for `cq.message` on line 114).

- [ ] **Step 5: Fix `serde_json::to_string(&data).unwrap()` in `batch_downloader.rs`**

In `book_bot/src/bots/approved_bot/services/batch_downloader.rs`, lines 64-65:

```rust
    let mut request = CLIENT
        .post(format!("{}/api/", &config::CONFIG.batch_downloader_url))
        .body(serde_json::to_string(&data).unwrap())
        .header("Authorization", &config::CONFIG.batch_downloader_api_key)
        .header("Content-Type", "application/json");
```

Replace with:

```rust
    let mut request = CLIENT
        .post(format!("{}/api/", &config::CONFIG.batch_downloader_url))
        .json(&data)
        .header("Authorization", &config::CONFIG.batch_downloader_api_key);
```

(`reqwest::RequestBuilder::json` serializes the body and sets `Content-Type: application/json` automatically. The `reqwest` crate already has the `json` feature enabled in `Cargo.toml`.)

- [ ] **Step 6: Build**

```
cargo build -p book_bot
```

Expected: success, no errors.

- [ ] **Step 7: Commit**

```bash
git add book_bot/src/config.rs \
        book_bot/src/main.rs \
        book_bot/src/bots_manager/closable_sender.rs \
        book_bot/src/bots/approved_bot/modules/random/mod.rs \
        book_bot/src/bots/approved_bot/services/batch_downloader.rs
git commit -m "fix(minor): WEBHOOK_PORT error msg; optional SENTRY_DSN; RwLock poison recovery; remove cq.data.unwrap(); use .json() in batch_downloader"
```

---

### Task 8: Document `panic = "abort"` decision in Cargo.toml (spec §decision)

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add comment to `[profile.release]`**

In `/Users/kurbezz/Projects/books_project/book_bot/Cargo.toml`, the `[profile.release]` block currently reads:

```toml
[profile.release]
opt-level = 3
debug = false
strip = true
lto = true
codegen-units = 1
panic = "abort"
```

Replace with:

```toml
[profile.release]
opt-level = 3
debug = false
strip = true
lto = true
codegen-units = 1
# panic = "abort": deliberate trade-off — reduces binary size and avoids
# stack-unwinding overhead. Acceptable only because the container orchestrator
# (e.g. Kubernetes) is configured to restart the process on exit.
# If restart guarantees are ever removed, revert to panic = "unwind".
panic = "abort"
```

- [ ] **Step 2: Commit**

```bash
git add Cargo.toml
git commit -m "chore: document panic=abort trade-off in Cargo.toml"
```

---

## Verification Checklist (run after all tasks)

- [ ] `cargo test -p book_bot` — all tests pass including the new ones
- [ ] `cargo clippy -p book_bot -- -D warnings` — no warnings
- [ ] `grep -rn "\.unwrap()\|\.expect(" book_bot/src/bots/approved_bot/services/book_cache/mod.rs` — only safe (compile-time/startup) unwraps remain
- [ ] `grep -n "page_index - 1\|page - 1" book_bot/src/bots/approved_bot/modules/annotations/mod.rs` — no matches
- [ ] `grep -n "content_size\.unwrap" book_bot/src/bots/approved_bot/modules/download/mod.rs` — no matches
- [ ] `grep -n "get_me.*unwrap\|username.*unwrap" book_bot/src/bots/approved_bot/mod.rs book_bot/src/bots/approved_bot/modules/utils/filter_command.rs book_bot/src/bots/approved_bot/modules/settings/mod.rs` — no matches
- [ ] `grep -n "message\.from\.unwrap\|get_token.*unwrap" book_bot/src/bots/bots_manager/mod.rs book_bot/src/bots/bots_manager/register.rs` — no matches
- [ ] `grep -n "cq\.data\.unwrap" book_bot/src/bots/approved_bot/modules/random/mod.rs` — no matches
- [ ] `grep -n "serde_json::to_string.*unwrap" book_bot/src/bots/approved_bot/services/batch_downloader.rs` — no matches
