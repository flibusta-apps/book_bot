# User-Settings Caching Consistency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the three inconsistent per-field user-settings caches (langs, file-name-lang, default-search) with one `UserSettings` cache that invalidates correctly, never caches errors, and coalesces concurrent loads; fix the donation-notification cache's expiry policy and operation order.

**Architecture:** A single `moka::future::Cache<UserId, Option<UserSettings>>` lives in `services/user_settings` and is loaded via `try_get_with` (coalesces concurrent misses, never caches an `Err`). The three existing public getters become thin wrappers over it. `create_or_update_user_settings` invalidates by user id, which now also fixes the file-name-lang staleness bug because there's only one cache to invalidate. Separately, `CHAT_DONATION_NOTIFICATIONS_CACHE` switches from `time_to_idle` to `time_to_live`, and `send_donation_notification`'s check→send→mark sequence is made atomic per-chat via the cache's `entry().or_try_insert_with()`, with the ordering logic pulled into a small pure function that's unit-testable without any network mocking.

**Tech Stack:** Rust, moka 0.12.10 (`future` feature, already a dependency — no Cargo.toml changes), tokio, teloxide.

## Global Constraints

- No new dependencies — moka 0.12.10 already provides `try_get_with` and the `Entry` API used here.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` must all pass (these are the exact CI commands from `.github/workflows/ci.yml`).
- Do not add a `Co-Authored-By` trailer to any commit in this repository.
- Keep the public function names and signatures used by callers unchanged (`get_user_or_default_lang_codes`, `get_user_file_name_lang_for`, `get_user_default_search`, `create_or_update_user_settings`) — only their implementation and, for `get_user_file_name_lang_for`, its module location change.

---

## Task 1: Unify the three user-settings caches into one

Fixes spec 7.1 (file-name-lang cache never invalidated), 7.2 (errors cached as valid values, inconsistent with the langs cache), 7.5 (three independent per-field mechanisms), and 7.6 (get-then-insert race for user settings — `try_get_with` coalesces concurrent loads).

**Files:**
- Modify: `book_bot/src/bots/approved_bot/services/user_settings/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/services/book_cache/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/download/mod.rs`
- Modify: `book_bot/src/bots_manager/mod.rs`

**Interfaces:**
- Produces (for Task 2, and for existing callers elsewhere in the codebase — unchanged from today): `pub async fn get_user_or_default_lang_codes(user_id: UserId) -> SmallVec<[SmartString; 3]>`, `pub async fn get_user_default_search(user_id: UserId) -> Option<DefaultSearchType>`, `pub async fn get_user_file_name_lang_for(user_id: Option<u64>) -> FileNameLang` — all now defined in `services/user_settings`.
- Produces: `pub static USER_SETTINGS_CACHE: LazyLock<Cache<UserId, Option<UserSettings>>>` in `services/user_settings`.

This task is a coordinated rename/move across 4 files that only compiles once every reference is updated — it cannot be split into independently-green sub-steps without leaving the crate in a non-compiling state, so the "test" for the wiring is `cargo build --workspace` after all edits, followed by a small unit test of the one genuinely new piece of logic (that `try_get_with` never caches an `Err`, which is the concrete mechanism fixing 7.2).

- [ ] **Step 1: Rewrite `services/user_settings/mod.rs` to own the single cache**

Replace the top of the file (imports) — current:

```rust
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use smallvec::{smallvec, SmallVec};
use smartstring::alias::String as SmartString;
use std::sync::LazyLock;
use teloxide::types::{ChatId, UserId};
use tracing::log;

use crate::{bots_manager::USER_LANGS_CACHE, config};
```

with:

```rust
use moka::future::Cache;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use smallvec::{smallvec, SmallVec};
use smartstring::alias::String as SmartString;
use std::sync::LazyLock;
use std::time::Duration;
use teloxide::types::{ChatId, UserId};
use tracing::log;

use crate::config;
```

Immediately after the `UserSettings` struct definition (after the closing `}` of that struct, before `pub async fn get_user_settings`), insert:

```rust
pub static USER_SETTINGS_CACHE: LazyLock<Cache<UserId, Option<UserSettings>>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(30 * 60))
        .max_capacity(4096)
        .build()
});

/// Loads the user's settings through `USER_SETTINGS_CACHE`. Concurrent
/// misses for the same user are coalesced into one HTTP request via
/// `try_get_with`. `Ok(None)` (the user has no settings yet) is a valid,
/// cacheable value; request errors are logged and never cached, so a
/// struggling user-settings service does not "stick" a stale default past
/// its own recovery.
async fn get_cached_user_settings(user_id: UserId) -> Option<UserSettings> {
    match USER_SETTINGS_CACHE
        .try_get_with(user_id, get_user_settings(user_id))
        .await
    {
        Ok(settings) => settings,
        Err(err) => {
            log::error!("{err:?}");
            None
        }
    }
}
```

Then replace `get_user_or_default_lang_codes` — current:

```rust
pub async fn get_user_or_default_lang_codes(user_id: UserId) -> SmallVec<[SmartString; 3]> {
    if let Some(cached_langs) = USER_LANGS_CACHE.get(&user_id).await {
        return cached_langs;
    }

    let default_lang_codes = smallvec!["ru".into(), "be".into(), "uk".into()];

    match get_user_settings(user_id).await {
        Ok(v) => {
            let langs: SmallVec<[SmartString; 3]> = match v {
                Some(v) => v.allowed_langs.into_iter().map(|lang| lang.code).collect(),
                None => return default_lang_codes,
            };
            USER_LANGS_CACHE.insert(user_id, langs.clone()).await;
            langs
        }
        Err(err) => {
            log::error!("{err:?}");
            default_lang_codes
        }
    }
}
```

with:

```rust
pub async fn get_user_or_default_lang_codes(user_id: UserId) -> SmallVec<[SmartString; 3]> {
    let default_lang_codes = smallvec!["ru".into(), "be".into(), "uk".into()];

    match get_cached_user_settings(user_id).await {
        Some(settings) => settings
            .allowed_langs
            .into_iter()
            .map(|lang| lang.code)
            .collect(),
        None => default_lang_codes,
    }
}
```

In `create_or_update_user_settings`, replace the single line:

```rust
    USER_LANGS_CACHE.invalidate(&user_id).await;
```

with:

```rust
    USER_SETTINGS_CACHE.invalidate(&user_id).await;
```

Replace `get_user_default_search` — current:

```rust
/// Returns user's default search type from API. None if not set or on error.
pub async fn get_user_default_search(user_id: UserId) -> Option<DefaultSearchType> {
    match get_user_settings(user_id).await {
        Ok(Some(s)) => s.default_search,
        _ => None,
    }
}
```

with:

```rust
/// Returns the user's default search type from the shared settings cache.
/// `None` if not set, the user has no settings, or the request failed.
pub async fn get_user_default_search(user_id: UserId) -> Option<DefaultSearchType> {
    get_cached_user_settings(user_id)
        .await
        .and_then(|settings| settings.default_search)
}
```

Immediately after `get_user_default_search`, add the two functions moved (and rewired) from `book_cache/mod.rs`:

```rust
/// Returns the user's `file_name_lang` setting via the shared settings
/// cache. On any error or missing user, returns the default (`Normalized`).
pub async fn get_user_file_name_lang(user_id: UserId) -> FileNameLang {
    get_cached_user_settings(user_id)
        .await
        .map(|settings| settings.file_name_lang)
        .unwrap_or_default()
}

/// Resolve `file_name_lang` for an `Option<u64>`. `None` means there is
/// no user context (e.g. an internal call) and we fall back to the
/// default, which is `Normalized`.
pub async fn get_user_file_name_lang_for(user_id: Option<u64>) -> FileNameLang {
    match user_id {
        Some(uid) => get_user_file_name_lang(UserId(uid)).await,
        None => FileNameLang::default(),
    }
}
```

- [ ] **Step 2: Remove the old per-field cache from `book_cache/mod.rs`**

Replace the top of the file — current:

```rust
use moka::future::Cache;
use reqwest::StatusCode;
use std::sync::LazyLock;
use std::time::Duration;
use teloxide::types::UserId;
use tracing::log;

use crate::{
    bots::approved_bot::modules::download::callback_data::DownloadQueryData,
    bots::approved_bot::services::{
        rate_limit::retry_on_429,
        user_settings::{get_user_settings, FileNameLang},
    },
    bots_manager::BotCache,
    config,
};

use self::types::{CachedMessage, DownloadFile};

pub mod types;

pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
});

pub static USER_FILE_NAME_LANG_CACHE: LazyLock<Cache<UserId, FileNameLang>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(30 * 60))
        .max_capacity(4096)
        .build()
});

/// Returns the user's `file_name_lang` setting, using the cache.
/// On any error or missing user, returns the default (`Normalized`).
pub async fn get_user_file_name_lang(user_id: UserId) -> FileNameLang {
    if let Some(cached) = USER_FILE_NAME_LANG_CACHE.get(&user_id).await {
        return cached;
    }

    let value = match get_user_settings(user_id).await {
        Ok(Some(s)) => s.file_name_lang,
        _ => FileNameLang::default(),
    };

    USER_FILE_NAME_LANG_CACHE.insert(user_id, value).await;
    value
}
```

with:

```rust
use reqwest::StatusCode;
use std::sync::LazyLock;
use tracing::log;

use crate::{
    bots::approved_bot::modules::download::callback_data::DownloadQueryData,
    bots::approved_bot::services::{
        rate_limit::retry_on_429,
        user_settings::{get_user_file_name_lang_for, FileNameLang},
    },
    bots_manager::BotCache,
    config,
};

use self::types::{CachedMessage, DownloadFile};

pub mod types;

pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
});
```

Then delete the `pub(crate) async fn get_user_file_name_lang_for` function near the bottom of the file (just above `download_file_by_link`) — current:

```rust
/// Resolve `file_name_lang` for an `Option<u64>`. `None` means there is
/// no user context (e.g. an internal call) and we fall back to the
/// default, which is `Normalized`.
pub(crate) async fn get_user_file_name_lang_for(user_id: Option<u64>) -> FileNameLang {
    match user_id {
        Some(uid) => get_user_file_name_lang(UserId(uid)).await,
        None => FileNameLang::default(),
    }
}

```

Delete it entirely (the two call sites in this same file, inside `get_cached_message` and `download_file`, already call `get_user_file_name_lang_for(user_id)` / `get_user_file_name_lang_for(user_id)` — leave those two call sites untouched; they now resolve to the imported function).

- [ ] **Step 3: Update the import in `modules/download/mod.rs`**

Current:

```rust
                book_cache::{
                    download_file, download_file_by_link, get_cached_message,
                    get_user_file_name_lang_for,
                    types::{CachedMessage, DownloadFile},
                },
                book_library::{
                    get_author_books_available_types, get_book, get_sequence_books_available_types,
                    get_translator_books_available_types,
                },
                donation_notifications::send_donation_notification,
                user_settings::{get_user_or_default_lang_codes, FileNameLang},
```

Replace with:

```rust
                book_cache::{
                    download_file, download_file_by_link, get_cached_message,
                    types::{CachedMessage, DownloadFile},
                },
                book_library::{
                    get_author_books_available_types, get_book, get_sequence_books_available_types,
                    get_translator_books_available_types,
                },
                donation_notifications::send_donation_notification,
                user_settings::{
                    get_user_file_name_lang_for, get_user_or_default_lang_codes, FileNameLang,
                },
```

- [ ] **Step 4: Remove `USER_LANGS_CACHE` from `bots_manager/mod.rs`**

Delete the static (current):

```rust
pub static USER_LANGS_CACHE: LazyLock<Cache<UserId, SmallVec<[SmartString; 3]>>> =
    LazyLock::new(|| {
        Cache::builder()
            .time_to_idle(Duration::from_secs(30 * 60))
            .max_capacity(4096)
            .build()
    });

```

Delete it entirely. Then remove the two now-unused imports:

```rust
use smartstring::alias::String as SmartString;
```

and

```rust
use smallvec::SmallVec;
```

(Both are used only by `USER_LANGS_CACHE`'s type; confirm with the build in Step 5 that nothing else in the file references `SmallVec`/`SmartString`.)

- [ ] **Step 5: Build the whole workspace**

Run: `cargo build --workspace`
Expected: builds cleanly with no errors and no new warnings about unused imports.

- [ ] **Step 6: Add a unit test for the core bug fix — errors are never cached**

This is the mechanism that fixes 7.2 (an error response no longer "sticks" as a trustworthy default). Add a test module at the bottom of `book_bot/src/bots/approved_bot/services/user_settings/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn try_get_with_never_caches_an_error() {
        let cache: Cache<u32, Option<u32>> = Cache::builder().build();
        let key = 1u32;

        let err_result = cache
            .try_get_with(key, async { Err::<Option<u32>, anyhow::Error>(anyhow::anyhow!("boom")) })
            .await;
        assert!(err_result.is_err());
        assert!(
            !cache.contains_key(&key),
            "an error must not be inserted into the cache"
        );

        let ok_result = cache
            .try_get_with(key, async { Ok::<_, anyhow::Error>(Some(42u32)) })
            .await
            .unwrap();
        assert_eq!(ok_result, Some(42));
        assert!(cache.contains_key(&key));
    }
}
```

- [ ] **Step 7: Run the new test**

Run: `cargo test -p book_bot user_settings::tests::try_get_with_never_caches_an_error`
Expected: `test ... ok`, 1 passed.

- [ ] **Step 8: Full verification and commit**

Run, in order:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Expected: all three succeed with no warnings/errors.

```bash
git add book_bot/src/bots/approved_bot/services/user_settings/mod.rs \
        book_bot/src/bots/approved_bot/services/book_cache/mod.rs \
        book_bot/src/bots/approved_bot/modules/download/mod.rs \
        book_bot/src/bots_manager/mod.rs
git commit -m "fix: unify user-settings caches so FileNameLang updates and errors invalidate correctly"
```

---

## Task 2: Fix donation-notification cache expiry and operation order

Fixes spec 7.3 (`time_to_idle` instead of `time_to_live`, effectively pausing the 24h schedule for active chats) and 7.4 (cache-before-check, mark-before-send, and no atomicity between the check and the insert).

**Files:**
- Modify: `book_bot/src/bots_manager/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/services/donation_notifications.rs`

**Interfaces:**
- Consumes: `is_need_donate_notifications(chat_id: ChatId, is_private: bool) -> anyhow::Result<bool>` and `mark_donate_notification_sent(chat_id: ChatId) -> anyhow::Result<()>` from `services/user_settings` (unchanged, already defined).
- Consumes: `support_command_handler(message: Message, bot: &CacheMe<Throttle<Bot>>) -> BotHandlerInternal` from `modules/support` (unchanged).
- Produces: `pub async fn send_donation_notification(bot: &CacheMe<Throttle<Bot>>, message: &MaybeInaccessibleMessage) -> BotHandlerInternal` (unchanged signature, only its body changes) — this is the only symbol other modules import from this file.

- [ ] **Step 1: Change the donation cache to `time_to_live`**

In `book_bot/src/bots_manager/mod.rs`, current:

```rust
pub static CHAT_DONATION_NOTIFICATIONS_CACHE: LazyLock<Cache<ChatId, ()>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(24 * 60 * 60))
        .max_capacity(4096)
        .build()
});
```

Replace with:

```rust
pub static CHAT_DONATION_NOTIFICATIONS_CACHE: LazyLock<Cache<ChatId, ()>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(24 * 60 * 60))
        .max_capacity(4096)
        .build()
});
```

- [ ] **Step 2: Write the failing tests for the operation-order fix**

Replace the whole contents of `book_bot/src/bots/approved_bot/services/donation_notifications.rs` with:

```rust
use std::future::Future;

use teloxide::{
    adaptors::{CacheMe, Throttle},
    types::{ChatId, MaybeInaccessibleMessage},
    Bot,
};

use crate::{
    bots::{approved_bot::modules::support::support_command_handler, BotHandlerInternal},
    bots_manager::CHAT_DONATION_NOTIFICATIONS_CACHE,
};

use super::user_settings::{is_need_donate_notifications, mark_donate_notification_sent};

/// Runs the check -> send -> mark sequence for one chat. `send` only runs
/// when `check` reports a notification is needed, and `mark` only runs
/// after `send` succeeds, so a failed Telegram send is never recorded as
/// "sent" server-side, and a failed check never suppresses a future retry.
async fn process_donation_notification<CheckFut, SendFut, MarkFut>(
    check: impl FnOnce() -> CheckFut,
    send: impl FnOnce() -> SendFut,
    mark: impl FnOnce() -> MarkFut,
) -> anyhow::Result<()>
where
    CheckFut: Future<Output = anyhow::Result<bool>>,
    SendFut: Future<Output = anyhow::Result<()>>,
    MarkFut: Future<Output = anyhow::Result<()>>,
{
    if check().await? {
        send().await?;
        mark().await?;
    }
    Ok(())
}

pub async fn send_donation_notification(
    bot: &CacheMe<Throttle<Bot>>,
    message: &MaybeInaccessibleMessage,
) -> BotHandlerInternal {
    let chat_id: ChatId = message.chat().id;
    let is_private = message.chat().is_private();

    CHAT_DONATION_NOTIFICATIONS_CACHE
        .entry(chat_id)
        .or_try_insert_with(process_donation_notification(
            move || is_need_donate_notifications(chat_id, is_private),
            move || async move {
                if let MaybeInaccessibleMessage::Regular(message) = message {
                    support_command_handler(*message.clone(), bot).await?;
                }
                Ok(())
            },
            move || mark_donate_notification_sent(chat_id),
        ))
        .await
        .map_err(|err| anyhow::anyhow!("{err:?}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::process_donation_notification;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StdMutex,
    };

    #[tokio::test]
    async fn mark_runs_only_after_a_successful_send() {
        let order: Arc<StdMutex<Vec<&'static str>>> = Arc::new(StdMutex::new(Vec::new()));

        let check_order = order.clone();
        let send_order = order.clone();
        let mark_order = order.clone();

        let result = process_donation_notification(
            move || async move {
                check_order.lock().unwrap().push("check");
                Ok(true)
            },
            move || async move {
                send_order.lock().unwrap().push("send");
                Ok(())
            },
            move || async move {
                mark_order.lock().unwrap().push("mark");
                Ok(())
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(*order.lock().unwrap(), vec!["check", "send", "mark"]);
    }

    #[tokio::test]
    async fn mark_is_not_called_when_send_fails() {
        let mark_called = Arc::new(AtomicBool::new(false));
        let mark_called_in_closure = mark_called.clone();

        let result = process_donation_notification(
            || async { Ok(true) },
            || async { Err(anyhow::anyhow!("telegram send failed")) },
            move || async move {
                mark_called_in_closure.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .await;

        assert!(result.is_err());
        assert!(!mark_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn send_and_mark_are_skipped_when_notification_not_needed() {
        let send_called = Arc::new(AtomicBool::new(false));
        let send_called_in_closure = send_called.clone();

        let result = process_donation_notification(
            || async { Ok(false) },
            move || async move {
                send_called_in_closure.store(true, Ordering::SeqCst);
                Ok(())
            },
            || async { panic!("mark should not be called") },
        )
        .await;

        assert!(result.is_ok());
        assert!(!send_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn propagates_check_errors_without_sending_or_marking() {
        let send_called = Arc::new(AtomicBool::new(false));
        let send_called_in_closure = send_called.clone();

        let result = process_donation_notification(
            || async { Err(anyhow::anyhow!("user-settings service down")) },
            move || async move {
                send_called_in_closure.store(true, Ordering::SeqCst);
                Ok(())
            },
            || async { panic!("mark should not be called") },
        )
        .await;

        assert!(result.is_err());
        assert!(!send_called.load(Ordering::SeqCst));
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p book_bot donation_notifications::tests`
Expected: 4 tests pass — `mark_runs_only_after_a_successful_send`, `mark_is_not_called_when_send_fails`, `send_and_mark_are_skipped_when_notification_not_needed`, `propagates_check_errors_without_sending_or_marking`.

- [ ] **Step 4: Full verification and commit**

Run, in order:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Expected: all three succeed with no warnings/errors.

```bash
git add book_bot/src/bots_manager/mod.rs \
        book_bot/src/bots/approved_bot/services/donation_notifications.rs
git commit -m "fix: correct donation-notification cache expiry and check/send/mark order"
```

---

## Manual acceptance scenarios (not automatable without a live/mocked user-settings service)

After both tasks are merged and deployed, verify by hand:

1. Change `FileNameLang` in `/settings` for a test account → immediately trigger a download for that account → confirm the file name uses the new format without restarting the process (spec acceptance criterion 1; fixed because `create_or_update_user_settings` now invalidates the same cache that `get_user_file_name_lang_for` reads from).
2. Point `user_settings_url` at an unreachable host, call any of the three getters (expect the default), restore the URL, wait for a successful call, and confirm the *real* value is now returned — not stuck on the default for the rest of the 30-minute TTL (spec acceptance criterion 2; fixed because `try_get_with` never inserts on `Err`).
