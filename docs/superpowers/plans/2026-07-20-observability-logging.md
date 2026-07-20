# Observability: log filtering, log levels, error classification, PII Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `RUST_LOG` actually control log verbosity, fix the case-sensitivity bug (and category placement) for `"Internal Server Error"` in Telegram error classification, and stop shipping full raw update bodies (potential PII) into ERROR logs/Sentry.

**Architecture:** Three independent, surgical fixes in the existing files named by the spec — no new modules, no new dependencies. Each fix gets its own commit and its own test.

**Tech Stack:** Rust, `tracing` / `tracing-subscriber` (`env-filter` feature, already in `Cargo.toml`), `axum`, standard `#[cfg(test)]` unit tests (`cargo test -p book_bot`).

## Global Constraints

- Spec source: `docs/specs/12-observability-logging.md`.
- `book_bot/Cargo.toml:28` already has `tracing-subscriber = { version = "0.3.19", features = ["env-filter"] }` — no dependency changes needed.
- Acceptance criteria (from the spec) that every task must satisfy collectively:
  1. `RUST_LOG=debug,tower_http=warn` takes effect without a rebuild.
  2. A unit test on `classify_telegram_error`: the string `"Internal Server Error"` is classified into the intended category.
  3. Update-parse errors do not put the full update body into ERROR logs/Sentry.
- **Note on spec item 12.4** ("Manager unavailability is logged at INFO", `book_bot/src/bots_manager/mod.rs:140-146`): already fixed by commit `62300d2` (`fix(bots_manager): log manager-fetch failures with a metric...`). Current code (`book_bot/src/bots_manager/mod.rs:201-211`, function `BotsManager::check`) already does:
  ```rust
  Err(err) => {
      log::error!("Failed to fetch bots from the manager API: {err:?}");
      record_manager_fetch_failure();
      return;
  }
  ```
  This already logs at ERROR with context text and increments the `bots_manager_fetch_failures_total` counter (tested in `manager_fetch_failure_increments_metric`, `book_bot/src/bots_manager/mod.rs:539-593`). **No task below touches this — it's a no-op verification, not a fix.**
- All file paths below are relative to the repo root `/Users/kurbezz/Projects/books_project/book_bot` (workspace root; the crate itself lives at `book_bot/book_bot/`). Run tests with `cargo test -p book_bot <filter>` from the workspace root.

---

### Task 1: `RUST_LOG` controls log verbosity at runtime (spec 12.1)

**Files:**
- Modify: `book_bot/src/main.rs:1-43`
- Test: inline `#[cfg(test)] mod tests` appended to `book_bot/src/main.rs`

**Interfaces:**
- Produces: `fn build_env_filter(rust_log: Option<String>) -> tracing_subscriber::filter::EnvFilter` (private, in `main.rs`) — not consumed by other tasks, but keep the name exact since the test module references it directly.

**Context:** `book_bot/src/main.rs:34-43` currently hard-codes `.with(filter::LevelFilter::INFO)`, so `RUST_LOG` is never read even though `tracing-subscriber`'s `env-filter` feature is compiled in. The spec's suggested one-line fix is `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))`. That call reads process-global env state directly, which makes it awkward to unit-test (parallel `cargo test` threads would race on `std::env::set_var`/`remove_var`, and mutating real env vars from a test is fragile). Instead, extract a small pure function that takes the already-read `RUST_LOG` value as a parameter — same runtime behavior, but testable without touching global state.

- [ ] **Step 1: Write the failing tests**

Append to `book_bot/src/main.rs` (this becomes the only content added by this task besides the two lines changed in `main()`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_info_when_rust_log_is_unset() {
        assert_eq!(build_env_filter(None).to_string(), "info");
    }

    #[test]
    fn honors_rust_log_override() {
        // `EnvFilter`'s `Display` impl reorders directives deterministically
        // (most-specific target first, bare default level last) rather than
        // preserving input order — verified empirically against
        // tracing-subscriber 0.3.19: `EnvFilter::try_new("debug,tower_http=warn")
        // .unwrap().to_string()` produces "tower_http=warn,debug", not
        // "debug,tower_http=warn".
        assert_eq!(
            build_env_filter(Some("debug,tower_http=warn".to_string())).to_string(),
            "tower_http=warn,debug"
        );
    }

    #[test]
    fn falls_back_to_info_when_rust_log_is_invalid() {
        // A bare word without `=level` (e.g. "not a valid directive!!") is
        // parsed as a target/module name with an implicit default level and
        // never fails — verified empirically. Only a directive with a
        // malformed level after `=` actually returns a parse error.
        assert_eq!(
            build_env_filter(Some("tower_http=notalevel".to_string())).to_string(),
            "info"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail (function doesn't exist yet)**

Run: `cargo test -p book_bot defaults_to_info_when_rust_log_is_unset honors_rust_log_override falls_back_to_info_when_rust_log_is_invalid`
Expected: compile error — `cannot find function \`build_env_filter\` in this scope`

- [ ] **Step 3: Implement `build_env_filter` and wire it into `main()`**

In `book_bot/src/main.rs`, replace lines 34-43:

```rust
    let sentry_layer = sentry_tracing::layer().event_filter(|md| match md.level() {
        &tracing::Level::ERROR => EventFilter::Event,
        _ => EventFilter::Ignore,
    });

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(filter::LevelFilter::INFO)
        .with(sentry_layer)
        .init();
```

with:

```rust
    let sentry_layer = sentry_tracing::layer().event_filter(|md| match md.level() {
        &tracing::Level::ERROR => EventFilter::Event,
        _ => EventFilter::Ignore,
    });

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(build_env_filter(std::env::var("RUST_LOG").ok()))
        .with(sentry_layer)
        .init();
```

Then add this function above `#[tokio::main]` (after the `use` block, before `mod bots_manager;` or after the `mod` lines — place it directly above `async fn main()`):

```rust
/// Builds the log filter from `RUST_LOG` (e.g. `debug,tower_http=warn`),
/// falling back to `info` when the variable is unset or fails to parse.
fn build_env_filter(rust_log: Option<String>) -> filter::EnvFilter {
    rust_log
        .and_then(|spec| filter::EnvFilter::try_new(spec).ok())
        .unwrap_or_else(|| filter::EnvFilter::new("info"))
}
```

Since `filter::LevelFilter` is no longer used, but `filter::EnvFilter` now is, the existing `use tracing_subscriber::filter;` import stays as-is (still needed).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p book_bot defaults_to_info_when_rust_log_is_unset honors_rust_log_override falls_back_to_info_when_rust_log_is_invalid`
Expected: `test result: ok. 3 passed; 0 failed`

- [ ] **Step 5: Confirm the whole crate still builds clean**

Run: `cargo build -p book_bot 2>&1 | grep -i warning`
Expected: no output (no new unused-import or dead-code warnings)

- [ ] **Step 6: Commit**

```bash
git add book_bot/src/main.rs
git commit -m "fix: make RUST_LOG control log verbosity instead of hard-coded INFO"
```

---

### Task 2: Fix case-sensitive `"internal Server Error"` match and give 500s their own category (spec 12.2)

**Files:**
- Modify: `book_bot/src/bots_manager/error_classification.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `ErrorCategory::Expected` still classifies `"Internal Server Error"` (any case) correctly, via a new private predicate `is_infra_error`. `classify_telegram_error` and `is_expected_telegram_error` keep their existing signatures — `book_bot/src/bots_manager/mod.rs` and `custom_error_handler.rs`/`internal.rs` (which already call `is_expected_telegram_error`) need no changes.

**Context:** `is_message_state_error` (`book_bot/src/bots_manager/error_classification.rs:76-84`) has `s.contains("internal Server Error")` — lowercase `i`, so it never matches Telegram's actual `"Internal Server Error"` (capital `I`). The spec asks for two things: (1) case-insensitive matching via `to_ascii_lowercase()`, and (2) moving 500s out of the "message state error" bucket into their own network/infra category, with a deliberate decision on Sentry routing. Telegram-side 5xx errors are transient and out of this service's control — the same reasoning `ErrorCategory::Expected`'s doc comment already gives for "network blips" — so this task classifies them as `Expected` (WARN, not sent to Sentry), matching `is_network_error`'s treatment of `"error decoding response body"` etc. There are currently no tests in this file at all, per `Explore` agent findings — this task starts one.

- [ ] **Step 1: Write the failing test**

Append to `book_bot/src/bots_manager/error_classification.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_server_error_is_classified_as_expected() {
        assert_eq!(
            classify_telegram_error("Internal Server Error"),
            ErrorCategory::Expected
        );
    }

    #[test]
    fn internal_server_error_matches_regardless_of_case() {
        assert_eq!(
            classify_telegram_error("internal server error"),
            ErrorCategory::Expected
        );
        assert_eq!(
            classify_telegram_error("INTERNAL SERVER ERROR"),
            ErrorCategory::Expected
        );
    }

    #[test]
    fn message_state_error_is_still_classified_as_expected() {
        assert_eq!(
            classify_telegram_error("Bad Request: message is not modified"),
            ErrorCategory::Expected
        );
    }

    #[test]
    fn unrecognized_error_is_classified_as_unexpected() {
        assert_eq!(
            classify_telegram_error("some genuinely new error shape"),
            ErrorCategory::Unexpected
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p book_bot internal_server_error_is_classified_as_expected`
Expected: FAIL — `"Internal Server Error"` classified as `Unexpected` (the buggy lowercase-`i` pattern in `is_message_state_error` never matches it)

- [ ] **Step 3: Fix the predicate and split out the infra category**

In `book_bot/src/bots_manager/error_classification.rs`, remove the buggy line from `is_message_state_error` (lines 76-84):

```rust
fn is_message_state_error(s: &str) -> bool {
    s.contains("message to edit not found")
        || s.contains("message is not modified")
        || s.contains("MESSAGE_ID_INVALID")
        || s.contains("text must be non-empty")
        || s.contains("Bad Request: message to be replied not found")
        || s.contains("migrated to a supergroup")
}
```

Add a new predicate below it:

```rust
/// Telegram-side 5xx responses. These are transient infrastructure
/// failures on Telegram's end (not something this service can act on),
/// so they're treated as Expected like other network blips — logged at
/// WARN, not sent to Sentry. Matched case-insensitively because Telegram
/// returns "Internal Server Error" with a capital I, and the exact casing
/// isn't a contract worth depending on.
fn is_infra_error(s: &str) -> bool {
    s.to_ascii_lowercase().contains("internal server error")
}
```

Update `classify_telegram_error` (lines 17-27) to include the new predicate:

```rust
pub fn classify_telegram_error(error_string: &str) -> ErrorCategory {
    if is_rate_limit_error(error_string)
        || is_network_error(error_string)
        || is_permission_error(error_string)
        || is_message_state_error(error_string)
        || is_infra_error(error_string)
    {
        ErrorCategory::Expected
    } else {
        ErrorCategory::Unexpected
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p book_bot error_classification`
Expected: `test result: ok. 4 passed; 0 failed`

- [ ] **Step 5: Commit**

```bash
git add book_bot/src/bots_manager/error_classification.rs
git commit -m "fix: match Internal Server Error case-insensitively, classify 5xx as its own infra category"
```

---

### Task 3: Stop shipping full update bodies into ERROR logs/Sentry, rewrite the misleading message (spec 12.3)

**Files:**
- Modify: `book_bot/src/bots_manager/utils.rs` (add `truncate_for_log` helper + its test)
- Modify: `book_bot/src/bots_manager/axum_server.rs:106-138`

**Interfaces:**
- Consumes: nothing from Tasks 1-2.
- Produces: `pub fn truncate_for_log(s: &str, max_chars: usize) -> String` in `book_bot::bots_manager::utils` — char-safe truncation (avoids panicking on multi-byte UTF-8 boundaries, unlike the existing byte-slicing `mask_token`, because update bodies contain arbitrary user text, not ASCII bot tokens).

**Context:** `book_bot/src/bots_manager/axum_server.rs:131-137` currently does:
```rust
Err(error) => {
    log::error!(
        "Cannot parse an update.\nError: {error:?}\nValue: {input}\n\
         This is a bug in teloxide-core, please open an issue here: \
         https://github.com/teloxide/teloxide/issues."
    );
}
```
Two problems: the message text is copied verbatim from teloxide-core's own internal error text and is wrong/confusing in this project's logs (it's not necessarily a teloxide bug — could be a malformed request), and the full raw update body (`{input}`) is logged at ERROR, which `main.rs`'s `sentry_layer.event_filter` (`&tracing::Level::ERROR => EventFilter::Event`) ships straight to Sentry — potentially including user message text, names, or other PII. Fix: log a body-free summary at ERROR (so Sentry still sees that parsing failed and why, satisfying alerting needs) and the truncated raw body at DEBUG only (which `EnvFilter::new("info")` — the default from Task 1 — filters out, and which the sentry layer's event_filter never forwards regardless of level). Also add the comment the spec asks for on the `UpdateKind::Error` re-parse block, which currently has no explanation.

- [ ] **Step 1: Write the failing test for `truncate_for_log`**

Read `book_bot/src/bots_manager/utils.rs` first to see the existing test module's exact style (it already has one, per the Explore agent's findings — `mask_token`/`mask_uri_path` tests at the bottom of the file). Add these tests inside that existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn truncate_for_log_leaves_short_strings_untouched() {
    assert_eq!(truncate_for_log("hello", 10), "hello");
}

#[test]
fn truncate_for_log_truncates_and_marks_long_strings() {
    assert_eq!(truncate_for_log("hello world", 5), "hello…");
}

#[test]
fn truncate_for_log_is_char_safe_on_multibyte_input() {
    // "héllo wörld" — truncating by raw byte offset 5 would land mid-character; char-based truncation must not panic.
    assert_eq!(truncate_for_log("héllo wörld", 5), "héllo…");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p book_bot truncate_for_log`
Expected: compile error — `cannot find function \`truncate_for_log\` in this scope`

- [ ] **Step 3: Implement `truncate_for_log` in `utils.rs`**

Add this function to `book_bot/src/bots_manager/utils.rs`, above its `#[cfg(test)]` module:

```rust
/// Truncates `s` to at most `max_chars` characters for safe logging,
/// appending `…` when truncated. Char-based (not byte-based) so it never
/// panics on multi-byte UTF-8 input, unlike `mask_token`'s byte slicing
/// (which is safe there only because bot tokens are ASCII).
pub fn truncate_for_log(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p book_bot truncate_for_log`
Expected: `test result: ok. 3 passed; 0 failed`

- [ ] **Step 5: Commit the helper**

```bash
git add book_bot/src/bots_manager/utils.rs
git commit -m "feat: add truncate_for_log helper for safely logging arbitrary user-supplied text"
```

- [ ] **Step 6: Fix the update-parse error site in `axum_server.rs`**

In `book_bot/src/bots_manager/axum_server.rs`, add the import (near the existing `use crate::bots_manager::utils::{mask_token, mask_uri_path};` at line 20):

```rust
use crate::bots_manager::utils::{mask_token, mask_uri_path, truncate_for_log};
```

Replace lines 106-138 (the whole `match serde_json::from_str::<Update>(&input) { ... }` block):

```rust
        match serde_json::from_str::<Update>(&input) {
            Ok(mut update) => {
                if let UpdateKind::Error(value) = &mut update.kind {
                    *value = serde_json::from_str(&input).unwrap_or_default();
                }

                match tx.try_send(Ok(update)) {
                    Ok(()) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        log::warn!(
                            "Update queue full for Bot(token={}); asking Telegram to retry",
                            mask_token(&token)
                        );
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        log::error!(
                            "Update channel closed for Bot(token={})",
                            mask_token(&token)
                        );
                        BOTS_ROUTES.remove(&token).await;
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                }
            }
            Err(error) => {
                log::error!(
                    "Cannot parse an update.\nError: {error:?}\nValue: {input}\n\
                     This is a bug in teloxide-core, please open an issue here: \
                     https://github.com/teloxide/teloxide/issues."
                );
            }
        };
```

with:

```rust
        match serde_json::from_str::<Update>(&input) {
            Ok(mut update) => {
                // teloxide-core parses updates it doesn't recognize into
                // `UpdateKind::Error(Value::default())`, discarding the raw
                // payload in the process. Re-parse the same input as a bare
                // `Value` here so downstream handlers still see the original
                // update body instead of an empty default.
                if let UpdateKind::Error(value) = &mut update.kind {
                    *value = serde_json::from_str(&input).unwrap_or_default();
                }

                match tx.try_send(Ok(update)) {
                    Ok(()) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        log::warn!(
                            "Update queue full for Bot(token={}); asking Telegram to retry",
                            mask_token(&token)
                        );
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        log::error!(
                            "Update channel closed for Bot(token={})",
                            mask_token(&token)
                        );
                        BOTS_ROUTES.remove(&token).await;
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                }
            }
            Err(error) => {
                // `error`'s Display can itself embed a fragment of the
                // offending field's raw value on a type-mismatch (e.g.
                // serde_json renders `invalid type: string "<value>",
                // expected i64` — verified empirically). Since this log is
                // ERROR-level and therefore Sentry-bound (main.rs's
                // event_filter), only log the content-free error category
                // here; the full error text and payload go to DEBUG only.
                log::error!(
                    "Failed to parse incoming Telegram update: {:?}",
                    error.classify()
                );
                log::debug!(
                    "Parse error detail: {error}\nMalformed update payload: {}",
                    truncate_for_log(&input, 2000)
                );
            }
        };
```

**Post-review addendum:** the task reviewer flagged that `serde_json::Error`'s `Display` can itself embed a fragment of the offending field's raw value on type-mismatch errors (verified: `serde_json::from_str` on a field expecting `i64` given a string produces `invalid type: string "<the string>", expected i64`). This was reported to the human, who chose to harden the fix (above) rather than accept the residual leak — `error.classify()` returns a content-free `Category` enum (`Io`/`Syntax`/`Data`/`Eof`) safe for the ERROR/Sentry-bound log, while the full error detail moves to the DEBUG line alongside the truncated body.

- [ ] **Step 7: Confirm the crate builds clean**

Run: `cargo build -p book_bot 2>&1 | grep -i warning`
Expected: no output

- [ ] **Step 8: Confirm no ERROR-level log site in this function still interpolates the raw body**

Run: `grep -n 'log::error!' book_bot/src/bots_manager/axum_server.rs`
Expected: three matches (`Cannot get a bot with token`, `Update channel closed`, `Failed to parse incoming Telegram update`) — none contain `{input}` or `Value: {input}`.

- [ ] **Step 9: Run the full test suite for this crate to check nothing else broke**

Run: `cargo test -p book_bot 2>&1 | tail -20`
Expected: `test result: ok.` with all previously-passing tests (160 baseline + the 3 new `main.rs` tests + the 4 new `error_classification.rs` tests + the 3 new `truncate_for_log` tests = 170 total) still passing, 0 failed.

- [ ] **Step 10: Commit**

```bash
git add book_bot/src/bots_manager/axum_server.rs
git commit -m "fix: stop logging full update bodies at ERROR/Sentry, rewrite misleading parse-error message"
```

---

## Final verification (run after all three tasks)

- [ ] Run: `cargo test -p book_bot 2>&1 | tail -5` — expect `test result: ok.`, 0 failed.
- [ ] Run: `cargo clippy -p book_bot -- -D warnings 2>&1 | tail -20` — expect no errors (project may or may not already be clippy-clean; if pre-existing warnings unrelated to this change appear, note them but don't block on them).
- [ ] Manually confirm acceptance criterion 1 end-to-end is plausible: `RUST_LOG=debug,tower_http=warn cargo run -p book_bot` requires the full `.env` (`TELEGRAM_BOT_API_ROOT`, `WEBHOOK_BASE_URL`, etc. — see `book_bot/src/config.rs`) which isn't part of this fix's scope; the unit tests on `build_env_filter` in Task 1 are the acceptance check for this criterion instead.
