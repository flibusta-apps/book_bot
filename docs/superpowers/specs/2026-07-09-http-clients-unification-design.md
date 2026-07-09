# Spec 08: Unifying the service-layer HTTP clients — client, URLs, statuses, retries

- **Priority:** medium
- **Effort:** M
- **Category:** reliability / maintainability
- **Source:** `docs/specs/08-http-clients-unification.md`

## Problem

### 8.1. Spaces in search queries are encoded as `+` in a path segment

`book_bot/src/bots/approved_bot/services/book_library/mod.rs:13-15,125,142,159,176`:
```rust
fn encode_path_segment(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
```
`form_urlencoded` is a query-string codec: space → `+`. In a path segment `+` is a literal character; the query "war and peace" is sent as `war+and+peace`. If the server currently decodes `+` into a space, that is a hidden dependency on non-standard behavior.

### 8.2. Four identical `reqwest::Client`s

`book_library/mod.rs:19-24`, `book_cache/mod.rs:23-28`, `user_settings/mod.rs:12-17`, `batch_downloader.rs:9-14` — verbatim-identical `LazyLock<reqwest::Client>` (timeout 30s): separate connection pools, 4 places to drift apart. None sets `connect_timeout` (connection establishment eats the shared 30s budget) or `pool_idle_timeout`.

### 8.3. Different status-handling contracts; error bodies are lost

`book_library/mod.rs:46-50` (404 → `Ok(None)`), `book_cache/mod.rs:108-112`/`157-159` (204 → `Ok(None)`), `user_settings/mod.rs:105-122` (204 checked after `error_for_status`, 404 → `Err`). As a result, `get_user_settings` returns `Err` for a brand-new user (no settings row yet, server responds 404), and `get_cached_user_settings` (`user_settings/mod.rs:128`) logs that via `log::error!` on every activity update for that user — a routine, expected case logged as an error, repeatedly. Everywhere `error_for_status()?` discards the response body — logs contain only the status and URL. Only `book_library` logs deserialization errors (`mod.rs:52-58`); the other three services propagate a JSON-parse failure via `?` with no log line of their own.

### 8.4. Manual string concatenation of URLs

`book_library/mod.rs:40`, `user_settings/mod.rs:107-241` (6 places: `get_user_settings`, `create_or_update_user_settings`, `get_langs`, `update_user_activity`, `is_need_donate_notifications`, `mark_donate_notification_sent`), `batch_downloader.rs:63,82-85`, `download/mod.rs:374-378,488-492` — `format!("{}{}", base_url, path)`; base URLs are `String`s in the config, not validated at startup; query params are glued into the string (`is_need_donate_notifications`'s `?is_private={is_private}`). The reference implementation already exists in the project: `book_cache` stores a `reqwest::Url` in the config and builds paths via `path_segments_mut` (`book_cache/mod.rs:30-38`, `build_cache_url`).

**Explicitly out of scope for this pass:** `bots_manager::bot_manager_client` builds its manager-API URL the same string-concatenation way (`config::CONFIG.manager_url`, a `String`). It isn't one of the four services named in the spec's problem list and has its own client/retry shape; left for a separate pass if it needs one.

### 8.5. `retry_on_429`: sleeping on the server-provided `retry_after` with no upper bound

`services/rate_limit.rs:126-147` — `wait = info.retry_after.max(backoff)`; `Retry-After: 3600` would hang the update handler for an hour (×3 retries). Also a leftover in the log message: `"(attempt {}/{}, max)"`.

### 8.6. Dead `#[serde(default)]` in batch_downloader

`batch_downloader.rs:33-49` — `#[serde(default = "default_normalized_true")]` on a Serialize-only struct has no effect (defaults apply only to deserialization); the default fn is marked `#[allow(dead_code)]`; the comment is misleading. (`create_task` already sends the body via `.json(&data)` — that part of the originally-suspected problem doesn't need a code change, only the dead attribute/function does.)

## Proposed solution

### One shared client

`services/mod.rs` (currently just `pub mod` lines) gains:

```rust
pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(5))
        .pool_idle_timeout(Duration::from_secs(90))
        .user_agent(concat!("book_bot/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("Failed to create HTTP client")
});
```

The four `pub static CLIENT` definitions in `book_library/mod.rs`, `book_cache/mod.rs`, `user_settings/mod.rs`, `batch_downloader.rs` are deleted; every call site switches to `services::HTTP_CLIENT`.

### URLs become `reqwest::Url`, validated at startup

`config.rs`: `book_server_url`, `user_settings_url`, `batch_downloader_url`, and `public_batch_downloader_url` change from `String` to `reqwest::Url`, parsed with `Url::parse(...).unwrap_or_else(|_| panic!("Cannot parse url from BOOK_SERVER_URL env variable"))` — the same pattern already used for `cache_server_url`. (`manager_url` stays a `String`; see the 8.4 out-of-scope note above.)

`services/mod.rs` gains a shared helper, promoted from `book_cache`'s existing `build_cache_url`:

```rust
pub fn build_url<'a>(base: &Url, segments: impl IntoIterator<Item = &'a str>) -> anyhow::Result<Url> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("URL has cannot-be-a-base scheme"))?
        .extend(segments);
    Ok(url)
}
```

`book_cache::build_cache_url` becomes a thin wrapper (`build_url(&config::CONFIG.cache_server_url, segments)`) or is removed in favor of calling `build_url` directly — implementer's call, no behavior difference.

### `book_library`: segments instead of formatted path strings

`_make_request` changes signature from `(url: &str, params) -> ...` to `(segments: &[&str], params) -> ...`, building the request URL via `build_url(&config::CONFIG.book_server_url, segments)`. This is a mechanical rewrite of every call site (`get_book`, `get_random_book_by_genre`, `get_random_author`, `get_random_sequence`, `get_genre_metas`, `get_genres`, `search_book`, `search_author`, `search_sequence`, `search_translator`, `get_book_annotation`, `get_author_annotation`, `get_author_books`, `get_translator_books`, `get_sequence_books`, `get_uploaded_books`, `get_author_books_available_types`, `get_translator_books_available_types`, `get_sequence_books_available_types`) — e.g.:

```rust
// before
_make_request(format!("/api/v1/books/{id}").as_str(), vec![]).await
// after
let id = id.to_string();
_make_request(&["api", "v1", "books", &id], vec![]).await
```

For the four `search_*` functions, the raw query text becomes its own segment instead of being pre-encoded with `encode_path_segment`:

```rust
// before
format!("/api/v1/books/search/{}", encode_path_segment(&query))
// after
&["api", "v1", "books", "search", &query]
```

`path_segments_mut().extend(...)` percent-encodes each segment per the URL path grammar (space → `%20`, not `+`), which is what fixes 8.1. `encode_path_segment` is deleted.

### `check_response`: one status/logging policy, two layers

`services/mod.rs` gains:

```rust
/// Splits a response into: the configured "empty" statuses -> None,
/// other 4xx/5xx -> Err (with a truncated body logged), else -> Ok(Some(resp)).
pub async fn check_status(
    resp: reqwest::Response,
    empty_statuses: &[StatusCode],
) -> anyhow::Result<Option<reqwest::Response>> {
    let status = resp.status();
    if empty_statuses.contains(&status) {
        return Ok(None);
    }
    if status.is_client_error() || status.is_server_error() {
        let url = resp.url().clone();
        let body = resp.text().await.unwrap_or_default();
        let truncated: String = body.chars().take(500).collect();
        log::error!("HTTP {status} from {url}: {truncated}");
        return Err(anyhow::anyhow!("HTTP {status} from {url}"));
    }
    Ok(Some(resp))
}

/// `check_status` plus JSON deserialization, with deserialization
/// failures logged uniformly (fixes the gap in 8.3 where only
/// book_library logged them).
pub async fn check_response<T: DeserializeOwned>(
    resp: reqwest::Response,
    empty_statuses: &[StatusCode],
) -> anyhow::Result<Option<T>> {
    let Some(resp) = check_status(resp, empty_statuses).await? else {
        return Ok(None);
    };
    let url = resp.url().clone();
    match resp.json::<T>().await {
        Ok(v) => Ok(Some(v)),
        Err(err) => {
            log::error!("Failed to deserialize response from {url}: {err:?}");
            Err(err.into())
        }
    }
}
```

Call sites:
- `book_library::_make_request` → `check_response::<T>(resp, &[StatusCode::NOT_FOUND])`.
- `user_settings::get_user_settings` → `check_response::<UserSettings>(resp, &[StatusCode::NOT_FOUND, StatusCode::NO_CONTENT])` — both statuses mean "no settings for this user" today; only 204 was handled correctly before, 404 was the logged-every-time bug this spec fixes.
- `book_cache::get_cached_message` → `check_response::<CachedMessage>(resp, &[StatusCode::NO_CONTENT])`.
- `book_cache::download_file` needs the raw `Response` (headers, byte stream) rather than JSON, so it calls `check_status(resp, &[StatusCode::NO_CONTENT])` and keeps reading headers off the `Some(resp)` it gets back.
- `book_cache::download_file_by_link` is left untouched: it collapses *any* non-200 status (success or error) into `Ok(None)` with no error path at all, which doesn't fit the empty-vs-error split `check_status` encodes, and the spec's problem list doesn't call out a bug in this function. Only its `CLIENT` reference is swapped for `services::HTTP_CLIENT`.
- `user_settings`'s other simple `error_for_status()?`-only call sites that deserialize a body (`create_or_update_user_settings` → `UserSettings`, `get_langs` → `Vec<Lang>`, `is_need_donate_notifications` → `bool`) → `check_response::<T>(resp, &[])` (no status means "empty" for these; any 4xx/5xx is a real error, now logged with its body).
- `update_user_activity` and `mark_donate_notification_sent` have no response body to deserialize (they return `Ok(())`), so they call `check_status(resp, &[]).await?` directly and discard the returned `Option<Response>` — `check_response::<()>` would wrongly try to JSON-deserialize an empty body.
- `batch_downloader`'s `create_task`/`get_task` → `check_response::<Task>(resp, &[])`.

### URL construction at the remaining call sites

- `user_settings`: all 6 string-concat call sites switch to `build_url(&config::CONFIG.user_settings_url, [...])`; `is_need_donate_notifications`'s glued-in `?is_private={is_private}` becomes `.query(&[("is_private", is_private.to_string())])`.
- `batch_downloader::create_task` → `build_url(&config::CONFIG.batch_downloader_url, ["api", ""])` (trailing empty segment preserves the current trailing slash); `get_task` → `build_url(..., ["api", "check_archive", task_id])`.
- `download/mod.rs`'s two link-building spots (`send_archive_link`, the post-completion download link) → `build_url(&config::CONFIG.public_batch_downloader_url, ["api", "download", &task.id])` and the `batch_downloader_url` equivalent, `.to_string()`'d at the call site since `download_file_by_link` keeps its existing `link: String` parameter (no signature change needed there).

### `retry_on_429`: bounded wait

`rate_limit.rs` gains `const MAX_WAIT: Duration = Duration::from_secs(30);`. Before sleeping, if `info.retry_after > MAX_WAIT`, return `Err` immediately (same error shape as the max-retries-exceeded case, so callers don't need a new branch) instead of calling `sleep`. The log line's stray `, max` literal is removed: `"Rate limited (attempt {}/{}), operation={:?}, waiting {}s"`.

### batch_downloader cleanup

Delete `default_normalized_true` and the `#[serde(default = "default_normalized_true")]` attribute on `CreateTaskData::normalized` (dead on a Serialize-only struct; `normalized` keeps its plain `pub bool` field, callers already always set it explicitly).

## Testing approach

Per-module `#[cfg(test)]` blocks, matching the existing convention (`rate_limit.rs`, `user_settings/mod.rs`'s `try_get_with_never_caches_an_error` test, `book_cache/mod.rs`'s header-decoding tests).

- `check_status`/`check_response`: unit tests using a local mock HTTP server (check existing dev-dependencies for a mock server crate already in use elsewhere in the workspace before adding one) covering: an empty-status response → `None`; a non-empty error status → `Err` with the body logged/included; a success response with a body that fails to deserialize → `Err`; a success response that deserializes → `Some(value)`.
- `build_url`: unit test confirming a segment containing a space/unicode round-trips to `%20`/percent-encoded form in the resulting `Url` (this is the direct regression test for 8.1, callable without any network).
- `retry_on_429`: extend the existing test module with a case where `retry_after` exceeds `MAX_WAIT` and assert it returns `Err` without ever calling `sleep` (structure the test so an unexpectedly-long sleep would fail the test via timeout, or refactor the wait-decision into a small pure function `fn effective_wait(retry_after, backoff, max_wait) -> Result<Duration, ()>` that's tested without any actual sleeping — implementer's choice, the pure-function split is cleaner and easier to test deterministically).
- `config::Config::load`'s URL-parsing panics are not separately unit-tested (matches the existing untested `cache_server_url` parse panic) — covered implicitly by the app failing to start with a bad env var, same as today.

## Acceptance criteria

- Searches with spaces/unicode reach the server with `%20` encoding (verified via the `build_url` unit test, and/or by logging the constructed URL against a real request).
- One client per process; `grep -rn 'reqwest::Client::builder'` inside `book_bot/src` finds no match outside `services/mod.rs`.
- A 500 (or any other non-"empty" 4xx/5xx) from any of the four services is logged with the status, URL, and the beginning of the response body.
- A brand-new user's settings lookup (404 from the user-settings service) no longer logs `log::error!` — verified by the `check_response` unit test's empty-status case for `[NOT_FOUND, NO_CONTENT]`.
- `Retry-After: 3600` does not block a handler longer than `MAX_WAIT` (30s) — verified by the `retry_on_429`/`effective_wait` unit test.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass (the exact CI commands from `.github/workflows/ci.yml`).

## Risks / notes

- Changing `book_server_url`/`user_settings_url`/`batch_downloader_url`/`public_batch_downloader_url` from `String` to `reqwest::Url` is a breaking change to `Config`'s public shape within the crate; every read site needs updating in the same commit/PR (the crate won't compile otherwise) — this is a coordinated, all-at-once change like the user-settings cache unification in spec 07, not independently-shippable steps.
- `download_file_by_link` currently treats "status != 200" as `None` (not "≥400 is an error, else success") — the design preserves that exact behavior rather than reinterpreting it, since the spec doesn't call out a bug there and changing it would be an unrelated behavior change.
- `public_batch_downloader_url` is never used as an actual request target in this codebase (only to build a link shown to the user in a Telegram message) — converting it to `Url` is for config consistency and startup validation, not because it fixes a request-building bug.
