# HTTP Clients Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the four duplicated `reqwest::Client`s, the manual string-concatenated URLs, and the three inconsistent HTTP status-handling contracts across `book_library`, `book_cache`, `user_settings`, and `batch_downloader` with one shared client and two shared helpers (`build_url`, `check_response`/`check_status`); bound `retry_on_429`'s wait time; delete a dead `#[serde(default)]` attribute.

**Architecture:** `services/mod.rs` gains `pub static HTTP_CLIENT` (one connection pool for the whole process) and two helpers: `build_url(base, segments)` (percent-encodes path segments via `Url::path_segments_mut`, fixing the `+`-for-space bug) and `check_response`/`check_status` (a single status-branching policy — configurable "empty" statuses map to `None`, other 4xx/5xx map to a logged `Err` with a truncated body, anything else deserializes). Each of the four services' base URL moves from `String` to `reqwest::Url` in `config.rs` (validated at startup, matching the existing `cache_server_url` pattern) and its call sites are rewritten against the two new helpers. `retry_on_429` gets a `MAX_WAIT` ceiling.

**Tech Stack:** Rust, reqwest 0.12 (already a dependency), url 2.5 (already a dependency), `http` crate (already a dev-dependency, used to build fake `reqwest::Response`s for tests without a mock server — see the existing pattern in `book_bot/src/bots_manager/bot_manager_client.rs`).

## Global Constraints

- No new dependencies — `http` is already a dev-dependency and is reused for response-mocking in tests; no mock-HTTP-server crate is added.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` must all pass (these are the exact CI commands from `.github/workflows/ci.yml`).
- Do not add a `Co-Authored-By` trailer to any commit in this repository.
- Keep every touched function's public name and signature unchanged unless this plan explicitly says otherwise (none do — `book_library`, `user_settings`, and `batch_downloader`'s public APIs are unchanged from the caller's perspective; only their internals change).
- `manager_url` (used by `bots_manager::bot_manager_client`) is out of scope for this plan — see the design doc's 8.4 note.

---

## Task 1: Shared `HTTP_CLIENT`, `build_url`, `check_status`, `check_response` in `services/mod.rs`; migrate `book_cache`

Fixes 8.2 (one client) for `book_cache` immediately, and lands the shared helpers every later task depends on. `book_cache` is migrated in this same task (rather than left for later) so nothing in `services/mod.rs` is unused dead code partway through the plan — `book_cache` already has a reference-quality `build_cache_url` helper doing almost exactly this, so folding it into the shared `build_url` is a natural first consumer.

**Files:**
- Modify: `book_bot/src/bots/approved_bot/services/mod.rs`
- Modify: `book_bot/src/bots/approved_bot/services/book_cache/mod.rs`

**Interfaces:**
- Produces (consumed by Tasks 3, 4, 5): `pub static HTTP_CLIENT: LazyLock<reqwest::Client>`, `pub fn build_url<'a>(base: &reqwest::Url, segments: impl IntoIterator<Item = &'a str>) -> anyhow::Result<reqwest::Url>`, `pub async fn check_status(response: reqwest::Response, empty_statuses: &[reqwest::StatusCode]) -> anyhow::Result<Option<reqwest::Response>>`, `pub async fn check_response<T: serde::de::DeserializeOwned>(response: reqwest::Response, empty_statuses: &[reqwest::StatusCode]) -> anyhow::Result<Option<T>>` — all in `crate::bots::approved_bot::services`.

- [ ] **Step 1: Rewrite `services/mod.rs`**

Replace the entire contents of `book_bot/src/bots/approved_bot/services/mod.rs` with:

```rust
pub mod batch_downloader;
pub mod book_cache;
pub mod book_library;
pub mod donation_notifications;
pub mod rate_limit;
pub mod user_settings;

use std::sync::LazyLock;
use std::time::Duration;

use reqwest::{StatusCode, Url};
use serde::de::DeserializeOwned;
use tracing::log;

pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(5))
        .pool_idle_timeout(Duration::from_secs(90))
        .user_agent(concat!("book_bot/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("Failed to create HTTP client")
});

/// Appends path segments to a base URL, letting the `url` crate handle
/// percent-encoding (e.g. a space becomes `%20`, never `+`).
pub fn build_url<'a>(
    base: &Url,
    segments: impl IntoIterator<Item = &'a str>,
) -> anyhow::Result<Url> {
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("URL has cannot-be-a-base scheme"))?
        .extend(segments);
    Ok(url)
}

/// Splits a response by status: any status in `empty_statuses` -> `Ok(None)`;
/// any other 4xx/5xx -> `Err` (status, URL, and a truncated response body
/// are logged); anything else -> `Ok(Some(response))`, with the body not
/// yet read so callers needing raw bytes/headers can still consume it.
pub async fn check_status(
    response: reqwest::Response,
    empty_statuses: &[StatusCode],
) -> anyhow::Result<Option<reqwest::Response>> {
    let status = response.status();

    if empty_statuses.contains(&status) {
        return Ok(None);
    }

    if status.is_client_error() || status.is_server_error() {
        let url = response.url().clone();
        let body = response.text().await.unwrap_or_default();
        let truncated: String = body.chars().take(500).collect();
        log::error!("HTTP {status} from {url}: {truncated}");
        return Err(anyhow::anyhow!("HTTP {status} from {url}"));
    }

    Ok(Some(response))
}

/// `check_status` plus JSON deserialization. A deserialization failure is
/// logged with the URL before being returned as `Err` (previously only
/// `book_library` logged this).
pub async fn check_response<T: DeserializeOwned>(
    response: reqwest::Response,
    empty_statuses: &[StatusCode],
) -> anyhow::Result<Option<T>> {
    let Some(response) = check_status(response, empty_statuses).await? else {
        return Ok(None);
    };

    let url = response.url().clone();
    match response.json::<T>().await {
        Ok(value) => Ok(Some(value)),
        Err(err) => {
            log::error!("Failed to deserialize response from {url}: {err:?}");
            Err(err.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with_status_and_body(status: u16, body: &str) -> reqwest::Response {
        let http_response = http::Response::builder()
            .status(status)
            .body(body.as_bytes().to_vec())
            .unwrap();
        reqwest::Response::from(http_response)
    }

    #[test]
    fn build_url_percent_encodes_spaces_as_percent20() {
        let base = Url::parse("https://example.com").unwrap();
        let url = build_url(&base, ["api", "v1", "books", "search", "war and peace"]).unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.com/api/v1/books/search/war%20and%20peace"
        );
    }

    #[test]
    fn build_url_percent_encodes_unicode_instead_of_leaving_it_raw() {
        let base = Url::parse("https://example.com").unwrap();
        let query = "война";
        let url = build_url(&base, ["api", "v1", "books", "search", query]).unwrap();
        assert!(
            !url.as_str().contains(query),
            "unicode must be percent-encoded, not left raw"
        );
        assert!(
            !url.as_str().contains('+'),
            "must not use query-string '+' encoding"
        );
    }

    #[tokio::test]
    async fn check_status_returns_none_for_empty_status() {
        let response = response_with_status_and_body(204, "");
        let result = check_status(response, &[StatusCode::NO_CONTENT])
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn check_status_errors_on_non_empty_error_status() {
        let response = response_with_status_and_body(500, "server exploded");
        let result = check_status(response, &[StatusCode::NO_CONTENT]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn check_status_passes_through_success() {
        let response = response_with_status_and_body(200, "ok");
        let result = check_status(response, &[StatusCode::NO_CONTENT])
            .await
            .unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn check_response_deserializes_success_body() {
        #[derive(serde::Deserialize)]
        struct Foo {
            a: u32,
        }

        let response = response_with_status_and_body(200, r#"{"a":1}"#);
        let result: Option<Foo> = check_response(response, &[StatusCode::NOT_FOUND])
            .await
            .unwrap();
        assert_eq!(result.unwrap().a, 1);
    }

    #[tokio::test]
    async fn check_response_returns_none_for_empty_status() {
        let response = response_with_status_and_body(404, "not found");
        let result: Option<serde_json::Value> = check_response(response, &[StatusCode::NOT_FOUND])
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn check_response_errors_on_bad_json() {
        let response = response_with_status_and_body(200, "not json");
        let result: anyhow::Result<Option<serde_json::Value>> =
            check_response(response, &[]).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run the new tests**

Run: `cargo test -p book_bot bots::approved_bot::services::tests`
Expected: 8 tests pass (`build_url_percent_encodes_spaces_as_percent20`, `build_url_percent_encodes_unicode_instead_of_leaving_it_raw`, `check_status_returns_none_for_empty_status`, `check_status_errors_on_non_empty_error_status`, `check_status_passes_through_success`, `check_response_deserializes_success_body`, `check_response_returns_none_for_empty_status`, `check_response_errors_on_bad_json`).

- [ ] **Step 3: Rewrite `book_cache/mod.rs`**

Replace the entire contents of `book_bot/src/bots/approved_bot/services/book_cache/mod.rs` with:

```rust
use reqwest::StatusCode;
use tracing::log;

use crate::{
    bots::approved_bot::modules::download::callback_data::DownloadQueryData,
    bots::approved_bot::services::{
        build_url, check_response, check_status,
        rate_limit::retry_on_429,
        user_settings::{get_user_file_name_lang_for, FileNameLang},
        HTTP_CLIENT,
    },
    bots_manager::BotCache,
    config,
};

use self::types::{CachedMessage, DownloadFile};

pub mod types;

pub async fn get_cached_message(
    download_data: &DownloadQueryData,
    bot_cache: BotCache,
    user_id: Option<u64>,
) -> anyhow::Result<Option<CachedMessage>> {
    let DownloadQueryData::DownloadData {
        book_id: id,
        file_type: format,
    } = download_data;

    let is_need_copy = bot_cache == BotCache::Cache;
    // The cache server now stores separate records per `normalized` mode.
    // Mirror the user's setting here so we hit the same record that
    // `download_file` would later request.
    let requested_original = matches!(
        get_user_file_name_lang_for(user_id).await,
        FileNameLang::Original
    );

    let mut url = build_url(
        &config::CONFIG.cache_server_url,
        ["api", "v1", &id.to_string(), format, ""],
    )?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("copy", &is_need_copy.to_string());
        if requested_original {
            q.append_pair("normalized", "false");
        }
    }

    let response = retry_on_429(user_id.is_some(), || {
        let mut req = HTTP_CLIENT
            .get(url.clone())
            .header("Authorization", &config::CONFIG.cache_server_api_key);

        if let Some(uid) = user_id {
            req = req.header("X-User-Id", uid.to_string());
        }

        req.send()
    })
    .await?;

    let cached: Option<CachedMessage> =
        check_response(response, &[StatusCode::NO_CONTENT]).await?;

    // The server echoes back the mode it actually used. If it disagrees
    // with what we requested, we're talking to an older / misconfigured
    // server and subsequent `download_file` calls will miss the cache.
    if let Some(cached) = &cached {
        if let Some(echo) = cached.is_normalized {
            let echoed_original = !echo;
            if echoed_original != requested_original {
                log::warn!(
                    "cache server echoed is_normalized={echo} for {id}/{format} \
                     but client requested original={requested_original}; \
                     cache mode may be inconsistent"
                );
            }
        }
    }

    Ok(cached)
}

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

pub async fn download_file(
    download_data: &DownloadQueryData,
    user_id: Option<u64>,
) -> anyhow::Result<Option<DownloadFile>> {
    let DownloadQueryData::DownloadData {
        book_id: id,
        file_type: format,
    } = download_data;

    // If the user has selected original (Cyrillic) file names, ask the
    // cache server not to transliterate. Default (Normalized / unknown)
    // matches the previous behavior — no query param is sent, the server
    // falls back to `normalized=true`.
    let original = matches!(
        get_user_file_name_lang_for(user_id).await,
        FileNameLang::Original
    );

    let mut url = build_url(
        &config::CONFIG.cache_server_url,
        ["api", "v1", "download", &id.to_string(), format, ""],
    )?;
    if original {
        url.query_pairs_mut().append_pair("normalized", "false");
    }

    let response = retry_on_429(user_id.is_some(), || {
        let mut req = HTTP_CLIENT
            .get(url.clone())
            .header("Authorization", &config::CONFIG.cache_server_api_key);

        if let Some(uid) = user_id {
            req = req.header("X-User-Id", uid.to_string());
        }

        req.send()
    })
    .await?;

    let Some(response) = check_status(response, &[StatusCode::NO_CONTENT]).await? else {
        return Ok(None);
    };

    let headers = response.headers();

    let filename = decode_b64_header(headers, "x-filename-b64")?;
    let caption = decode_b64_header(headers, "x-caption-b64")?;

    Ok(Some(DownloadFile {
        response,
        filename,
        caption,
    }))
}

pub async fn download_file_by_link(
    filename: &str,
    link: String,
) -> anyhow::Result<Option<DownloadFile>> {
    let response = HTTP_CLIENT.get(link).send().await?;

    if response.status() != StatusCode::OK {
        return Ok(None);
    };

    Ok(Some(DownloadFile {
        response,
        filename: filename.to_string(),
        caption: "".to_string(),
    }))
}

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

This deletes the local `pub static CLIENT`, the local `build_cache_url`, and the manual `if response.status() == StatusCode::NO_CONTENT { ... } let response = response.error_for_status()?;` branches — replaced by `check_response`/`check_status`. `download_file_by_link` is otherwise untouched (only its `CLIENT` reference becomes `HTTP_CLIENT`) — it collapses any non-200 status into `None` with no error path, which doesn't fit the empty-vs-error split, and the spec doesn't call out a bug there.

- [ ] **Step 4: Build and run book_cache's existing tests**

Run: `cargo build --workspace && cargo test -p book_bot bots::approved_bot::services::book_cache`
Expected: builds cleanly; the 4 existing `decode_b64_header` tests pass.

- [ ] **Step 5: Full verification and commit**

Run, in order:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Expected: all three succeed with no warnings/errors.

```bash
git add book_bot/src/bots/approved_bot/services/mod.rs \
        book_bot/src/bots/approved_bot/services/book_cache/mod.rs
git commit -m "refactor: add shared HTTP client and status/URL helpers, migrate book_cache"
```

---

## Task 2: Bound `retry_on_429`'s wait time

Fixes 8.5. Fully independent of Task 1 — can be done in any order relative to it.

**Files:**
- Modify: `book_bot/src/bots/approved_bot/services/rate_limit.rs`

**Interfaces:**
- Produces: `const MAX_WAIT: Duration = Duration::from_secs(30);` (not consumed by other files — it's an internal cap on `retry_on_429`'s own behavior).

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` block at the bottom of `book_bot/src/bots/approved_bot/services/rate_limit.rs` (after `max_retries_constant`):

```rust
    #[test]
    fn max_wait_constant() {
        assert_eq!(MAX_WAIT, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn retry_on_429_aborts_immediately_when_retry_after_exceeds_max_wait() {
        let make_response = || async {
            let http_response = http::Response::builder()
                .status(429)
                .header("Retry-After", "3600")
                .body(Vec::<u8>::new())
                .unwrap();
            Ok::<_, reqwest::Error>(reqwest::Response::from(http_response))
        };

        let start = std::time::Instant::now();
        let result = retry_on_429(true, make_response).await;
        let elapsed = start.elapsed();

        assert!(result.is_err());
        assert!(
            elapsed < Duration::from_secs(1),
            "must not sleep when retry_after exceeds MAX_WAIT, took {elapsed:?}"
        );
    }
```

- [ ] **Step 2: Run the new tests to verify they fail**

Run: `cargo test -p book_bot bots::approved_bot::services::rate_limit::tests::max_wait_constant bots::approved_bot::services::rate_limit::tests::retry_on_429_aborts_immediately_when_retry_after_exceeds_max_wait`
Expected: FAIL — `max_wait_constant` fails to compile (`MAX_WAIT` doesn't exist yet); once that's added temporarily to check the second test, `retry_on_429_aborts_immediately_when_retry_after_exceeds_max_wait` would time out sleeping ~3600s if run for real, so don't run it standalone against the old code — the compile failure on `MAX_WAIT` is sufficient confirmation this step precedes the implementation.

- [ ] **Step 3: Add `MAX_WAIT` and use it**

In `book_bot/src/bots/approved_bot/services/rate_limit.rs`, after the existing constant:

```rust
/// Maximum number of retry attempts for a rate-limited request.
const MAX_RETRIES: u32 = 3;
```

add:

```rust
/// Upper bound on how long we'll sleep for a single retry, regardless of
/// what the server's `Retry-After` says — protects against a server
/// requesting an hour-long wait and stalling the calling handler.
const MAX_WAIT: Duration = Duration::from_secs(30);
```

Then replace this block inside `retry_on_429`:

```rust
        let backoff = Duration::from_secs(2u64.saturating_pow(attempt));
        let wait = info.retry_after.max(backoff);

        warn!(
            "Rate limited (attempt {}/{}, max), operation={:?}, waiting {}s",
            attempt + 1,
            max_attempts,
            info.operation,
            wait.as_secs(),
        );

        if attempt + 1 >= max_attempts {
            return Err(anyhow::anyhow!(
                "rate_limit_exceeded: operation={}, retry_after={}s",
                info.operation
                    .map(|op| op.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                info.retry_after.as_secs(),
            ));
        }

        sleep(wait).await;
        attempt += 1;
```

with:

```rust
        if info.retry_after > MAX_WAIT {
            warn!(
                "Rate limited: server-requested retry_after={}s exceeds MAX_WAIT={}s, \
                 aborting instead of sleeping, operation={:?}",
                info.retry_after.as_secs(),
                MAX_WAIT.as_secs(),
                info.operation,
            );
            return Err(anyhow::anyhow!(
                "rate_limit_exceeded (retry_after too long): operation={}, retry_after={}s",
                info.operation
                    .map(|op| op.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                info.retry_after.as_secs(),
            ));
        }

        let backoff = Duration::from_secs(2u64.saturating_pow(attempt));
        let wait = info.retry_after.max(backoff);

        warn!(
            "Rate limited (attempt {}/{}), operation={:?}, waiting {}s",
            attempt + 1,
            max_attempts,
            info.operation,
            wait.as_secs(),
        );

        if attempt + 1 >= max_attempts {
            return Err(anyhow::anyhow!(
                "rate_limit_exceeded: operation={}, retry_after={}s",
                info.operation
                    .map(|op| op.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                info.retry_after.as_secs(),
            ));
        }

        sleep(wait).await;
        attempt += 1;
```

(The only textual change to the `warn!` format string besides reordering is dropping the stray `, max` — it now reads `"Rate limited (attempt {}/{})"`.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p book_bot bots::approved_bot::services::rate_limit::tests`
Expected: all tests in the module pass, including the two new ones, and `retry_on_429_aborts_immediately_when_retry_after_exceeds_max_wait` completes in well under a second (proving it never called `sleep`).

- [ ] **Step 5: Full verification and commit**

Run, in order:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Expected: all three succeed with no warnings/errors.

```bash
git add book_bot/src/bots/approved_bot/services/rate_limit.rs
git commit -m "fix: bound retry_on_429's wait to MAX_WAIT, fix leftover log text"
```

---

## Task 3: `book_library` — `Url` config, segment-based paths, shared client/status handling

Fixes 8.1 (the `+`-for-space bug) and folds `book_library` into 8.2/8.3/8.4. Depends on Task 1 (`build_url`, `check_response`, `HTTP_CLIENT`).

**Files:**
- Modify: `book_bot/src/config.rs`
- Modify: `book_bot/src/bots/approved_bot/services/book_library/mod.rs`

**Interfaces:**
- Consumes: `build_url`, `check_response`, `HTTP_CLIENT` from `crate::bots::approved_bot::services` (Task 1).
- Produces: no change to `book_library`'s public function names/signatures — every existing caller elsewhere in the codebase keeps working unchanged.

- [ ] **Step 1: Convert `book_server_url` to `reqwest::Url` in `config.rs`**

In `book_bot/src/config.rs`, change:

```rust
    pub book_server_url: String,
    pub book_server_api_key: String,
```

to:

```rust
    pub book_server_url: reqwest::Url,
    pub book_server_api_key: String,
```

and change:

```rust
            book_server_url: get_env("BOOK_SERVER_URL"),
            book_server_api_key: get_env("BOOK_SERVER_API_KEY"),
```

to:

```rust
            book_server_url: reqwest::Url::parse(&get_env("BOOK_SERVER_URL"))
                .unwrap_or_else(|_| panic!("Cannot parse url from BOOK_SERVER_URL env variable")),
            book_server_api_key: get_env("BOOK_SERVER_API_KEY"),
```

- [ ] **Step 2: Rewrite `book_library/mod.rs`**

Replace the entire contents of `book_bot/src/bots/approved_bot/services/book_library/mod.rs` with:

```rust
pub mod formatters;
pub mod types;

use smartstring::alias::String as SmartString;

use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use smallvec::SmallVec;

use crate::{
    bots::approved_bot::services::{build_url, check_response, HTTP_CLIENT},
    config,
};

use self::types::Empty;

fn get_allowed_langs_params(
    allowed_langs: &SmallVec<[SmartString; 3]>,
) -> Vec<(&'static str, SmartString)> {
    allowed_langs
        .into_iter()
        .map(|lang| ("allowed_langs", lang.clone()))
        .collect()
}

async fn _make_request<T>(
    segments: &[&str],
    params: Vec<(&str, SmartString)>,
) -> anyhow::Result<Option<T>>
where
    T: DeserializeOwned,
{
    let url = build_url(&config::CONFIG.book_server_url, segments.iter().copied())?;

    let response = HTTP_CLIENT
        .get(url)
        .query(&params)
        .header("Authorization", &config::CONFIG.book_server_api_key)
        .send()
        .await?;

    check_response(response, &[StatusCode::NOT_FOUND]).await
}

pub async fn get_book(id: u32) -> anyhow::Result<Option<types::Book>> {
    _make_request(&["api", "v1", "books", &id.to_string()], vec![]).await
}

pub async fn get_random_book_by_genre(
    allowed_langs: SmallVec<[SmartString; 3]>,
    genre: Option<u32>,
) -> anyhow::Result<Option<types::Book>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    if let Some(v) = genre {
        params.push(("genre", v.to_string().into()));
    }

    _make_request(&["api", "v1", "books", "random"], params).await
}

pub async fn get_random_book(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Book>> {
    get_random_book_by_genre(allowed_langs, None).await
}

pub async fn get_random_author(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Author>> {
    let params = get_allowed_langs_params(&allowed_langs);

    _make_request(&["api", "v1", "authors", "random"], params).await
}

pub async fn get_random_sequence(
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Sequence>> {
    let params = get_allowed_langs_params(&allowed_langs);

    _make_request(&["api", "v1", "sequences", "random"], params).await
}

pub async fn get_genre_metas() -> anyhow::Result<Option<Vec<String>>> {
    _make_request(&["api", "v1", "genres", "metas"], vec![]).await
}

pub async fn get_genres(
    meta: SmartString,
) -> anyhow::Result<Option<types::Page<types::Genre, Empty>>> {
    let params = vec![("meta", meta)];

    _make_request(&["api", "v1", "genres"], params).await
}

const PAGE_SIZE: &str = "5";

pub async fn search_book(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::SearchBook, Empty>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "books", "search", &query], params).await
}

pub async fn search_author(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::Author, Empty>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "authors", "search", &query], params).await
}

pub async fn search_sequence(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::Sequence, Empty>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "sequences", "search", &query], params).await
}

pub async fn search_translator(
    query: String,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::Translator, Empty>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "translators", "search", &query], params).await
}

pub async fn get_book_annotation(id: u32) -> anyhow::Result<Option<types::BookAnnotation>> {
    _make_request(
        &["api", "v1", "books", &id.to_string(), "annotation"],
        vec![],
    )
    .await
}

pub async fn get_author_annotation(id: u32) -> anyhow::Result<Option<types::AuthorAnnotation>> {
    _make_request(
        &["api", "v1", "authors", &id.to_string(), "annotation"],
        vec![],
    )
    .await
}

pub async fn get_author_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::AuthorBook, types::BookAuthor>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(&["api", "v1", "authors", &id.to_string(), "books"], params).await
}

pub async fn get_translator_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::TranslatorBook, types::BookTranslator>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(
        &["api", "v1", "translators", &id.to_string(), "books"],
        params,
    )
    .await
}

pub async fn get_sequence_books(
    id: u32,
    page: u32,
    allowed_langs: SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<types::Page<types::SequenceBook, types::Sequence>>> {
    let mut params = get_allowed_langs_params(&allowed_langs);

    params.push(("page", page.to_string().into()));
    params.push(("size", PAGE_SIZE.to_string().into()));

    _make_request(
        &["api", "v1", "sequences", &id.to_string(), "books"],
        params,
    )
    .await
}

pub async fn get_uploaded_books(
    page: u32,
    uploaded_gte: SmartString,
    uploaded_lte: SmartString,
) -> anyhow::Result<Option<types::Page<types::SearchBook, Empty>>> {
    let params = vec![
        ("page", page.to_string().into()),
        ("size", PAGE_SIZE.to_string().into()),
        ("uploaded_gte", uploaded_gte),
        ("uploaded_lte", uploaded_lte),
        ("is_deleted", "false".into()),
    ];

    _make_request(&["api", "v1", "books"], params).await
}

pub async fn get_author_books_available_types(
    id: u32,
    allowed_langs: &SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<Vec<String>>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request(
        &["api", "v1", "authors", &id.to_string(), "available_types"],
        params,
    )
    .await
}

pub async fn get_translator_books_available_types(
    id: u32,
    allowed_langs: &SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<Vec<String>>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request(
        &["api", "v1", "translators", &id.to_string(), "available_types"],
        params,
    )
    .await
}

pub async fn get_sequence_books_available_types(
    id: u32,
    allowed_langs: &SmallVec<[SmartString; 3]>,
) -> anyhow::Result<Option<Vec<String>>> {
    let params = get_allowed_langs_params(allowed_langs);

    _make_request(
        &["api", "v1", "sequences", &id.to_string(), "available_types"],
        params,
    )
    .await
}
```

This deletes `encode_path_segment` and the local `pub static CLIENT` — every path is now built from segments via `build_url`, so `path_segments_mut` percent-encodes each one (space -> `%20`, unicode -> percent-encoded, never `+`).

- [ ] **Step 3: Build and run book_library's existing tests**

Run: `cargo build --workspace && cargo test -p book_bot bots::approved_bot::services::book_library`
Expected: builds cleanly; the existing tests in `book_library/types.rs` still pass (they're unaffected by this change — this confirms no regression in the sibling module).

- [ ] **Step 4: Full verification and commit**

Run, in order:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Expected: all three succeed with no warnings/errors.

```bash
git add book_bot/src/config.rs \
        book_bot/src/bots/approved_bot/services/book_library/mod.rs
git commit -m "fix: encode book_library search queries as path segments, not query-string plus"
```

---

## Task 4: `user_settings` — `Url` config, shared client/status handling, fix the 404-logged-as-error bug

Fixes 8.3's user-settings half (a brand-new user's 404 is no longer logged as an error on every activity update) and folds `user_settings` into 8.2/8.4. Depends on Task 1.

**Files:**
- Modify: `book_bot/src/config.rs`
- Modify: `book_bot/src/bots/approved_bot/services/user_settings/mod.rs`

**Interfaces:**
- Consumes: `build_url`, `check_response`, `check_status`, `HTTP_CLIENT` from `crate::bots::approved_bot::services` (Task 1).
- Produces: no change to `user_settings`'s public function names/signatures.

- [ ] **Step 1: Convert `user_settings_url` to `reqwest::Url` in `config.rs`**

In `book_bot/src/config.rs`, change:

```rust
    pub user_settings_url: String,
    pub user_settings_api_key: String,
```

to:

```rust
    pub user_settings_url: reqwest::Url,
    pub user_settings_api_key: String,
```

and change:

```rust
            user_settings_url: get_env("USER_SETTINGS_URL"),
            user_settings_api_key: get_env("USER_SETTINGS_API_KEY"),
```

to:

```rust
            user_settings_url: reqwest::Url::parse(&get_env("USER_SETTINGS_URL"))
                .unwrap_or_else(|_| panic!("Cannot parse url from USER_SETTINGS_URL env variable")),
            user_settings_api_key: get_env("USER_SETTINGS_API_KEY"),
```

- [ ] **Step 2: Write the failing test for the 404 fix**

Add to the existing `#[cfg(test)] mod tests` block at the bottom of `book_bot/src/bots/approved_bot/services/user_settings/mod.rs` (after `try_get_with_never_caches_an_error`):

```rust
    #[tokio::test]
    async fn a_404_from_the_user_settings_service_is_not_an_error() {
        use crate::bots::approved_bot::services::check_response;

        let http_response = http::Response::builder()
            .status(404)
            .body(Vec::<u8>::new())
            .unwrap();
        let response = reqwest::Response::from(http_response);

        let result: anyhow::Result<Option<UserSettings>> =
            check_response(response, &[StatusCode::NOT_FOUND, StatusCode::NO_CONTENT]).await;

        assert!(result.is_ok(), "404 must not be an Err");
        assert!(
            result.unwrap().is_none(),
            "404 must mean 'no settings for this user', i.e. None"
        );
    }
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p book_bot bots::approved_bot::services::user_settings::tests::a_404_from_the_user_settings_service_is_not_an_error`
Expected: FAIL to compile (`check_response` doesn't exist in this crate's import path relative to this file yet, or the module hasn't wired it in) — confirms the test exercises code not yet present.

- [ ] **Step 4: Rewrite `user_settings/mod.rs`'s imports and network functions**

At the top of `book_bot/src/bots/approved_bot/services/user_settings/mod.rs`, replace:

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

use crate::{
    bots::approved_bot::services::{build_url, check_response, check_status, HTTP_CLIENT},
    config,
};
```

Delete the `pub static CLIENT` block:

```rust
pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to create HTTP client")
});
```

Replace `get_user_settings`:

```rust
pub async fn get_user_settings(user_id: UserId) -> anyhow::Result<Option<UserSettings>> {
    let response = CLIENT
        .get(format!(
            "{}/users/{}",
            &config::CONFIG.user_settings_url,
            user_id
        ))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    if response.status() == StatusCode::NO_CONTENT {
        return Ok(None);
    }

    Ok(Some(response.json::<UserSettings>().await?))
}
```

with:

```rust
pub async fn get_user_settings(user_id: UserId) -> anyhow::Result<Option<UserSettings>> {
    let url = build_url(
        &config::CONFIG.user_settings_url,
        ["users", &user_id.to_string()],
    )?;

    let response = HTTP_CLIENT
        .get(url)
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_response(response, &[StatusCode::NOT_FOUND, StatusCode::NO_CONTENT]).await
}
```

Replace `create_or_update_user_settings`'s request-sending tail (everything from `let response = CLIENT` to the end of the function):

```rust
    let response = CLIENT
        .post(format!("{}/users/", &config::CONFIG.user_settings_url))
        .body(body.to_string())
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .header("Content-Type", "application/json")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<UserSettings>().await?)
}
```

with:

```rust
    let url = build_url(&config::CONFIG.user_settings_url, ["users", ""])?;

    let response = HTTP_CLIENT
        .post(url)
        .body(body.to_string())
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .header("Content-Type", "application/json")
        .send()
        .await?;

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("user-settings service returned an empty response"))
}
```

Replace `get_langs`:

```rust
pub async fn get_langs() -> anyhow::Result<Vec<Lang>> {
    let response = CLIENT
        .get(format!("{}/languages/", &config::CONFIG.user_settings_url))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<Vec<Lang>>().await?)
}
```

with:

```rust
pub async fn get_langs() -> anyhow::Result<Vec<Lang>> {
    let url = build_url(&config::CONFIG.user_settings_url, ["languages", ""])?;

    let response = HTTP_CLIENT
        .get(url)
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("user-settings service returned an empty response"))
}
```

Replace `update_user_activity`:

```rust
pub async fn update_user_activity(user_id: UserId) -> anyhow::Result<()> {
    CLIENT
        .post(format!(
            "{}/users/{user_id}/update_activity",
            &config::CONFIG.user_settings_url
        ))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
```

with:

```rust
pub async fn update_user_activity(user_id: UserId) -> anyhow::Result<()> {
    let url = build_url(
        &config::CONFIG.user_settings_url,
        ["users", &user_id.to_string(), "update_activity"],
    )?;

    let response = HTTP_CLIENT
        .post(url)
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_status(response, &[]).await?;

    Ok(())
}
```

Replace `is_need_donate_notifications`:

```rust
pub async fn is_need_donate_notifications(
    chat_id: ChatId,
    is_private: bool,
) -> anyhow::Result<bool> {
    let response = CLIENT
        .get(format!(
            "{}/donate_notifications/{chat_id}/is_need_send?is_private={is_private}",
            &config::CONFIG.user_settings_url
        ))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<bool>().await?)
}
```

with:

```rust
pub async fn is_need_donate_notifications(
    chat_id: ChatId,
    is_private: bool,
) -> anyhow::Result<bool> {
    let url = build_url(
        &config::CONFIG.user_settings_url,
        ["donate_notifications", &chat_id.to_string(), "is_need_send"],
    )?;

    let response = HTTP_CLIENT
        .get(url)
        .query(&[("is_private", is_private.to_string())])
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("user-settings service returned an empty response"))
}
```

Replace `mark_donate_notification_sent`:

```rust
pub async fn mark_donate_notification_sent(chat_id: ChatId) -> anyhow::Result<()> {
    CLIENT
        .post(format!(
            "{}/donate_notifications/{chat_id}",
            &config::CONFIG.user_settings_url
        ))
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
```

with:

```rust
pub async fn mark_donate_notification_sent(chat_id: ChatId) -> anyhow::Result<()> {
    let url = build_url(
        &config::CONFIG.user_settings_url,
        ["donate_notifications", &chat_id.to_string()],
    )?;

    let response = HTTP_CLIENT
        .post(url)
        .header("Authorization", &config::CONFIG.user_settings_api_key)
        .send()
        .await?;

    check_status(response, &[]).await?;

    Ok(())
}
```

Every other function in the file (`DefaultSearchType`, `FileNameLang`, `Lang`, `UserSettings`, `USER_SETTINGS_CACHE`, `get_cached_user_settings`, `get_user_or_default_lang_codes`, `get_user_default_search`, `get_user_file_name_lang`, `get_user_file_name_lang_for`) is unchanged.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p book_bot bots::approved_bot::services::user_settings::tests`
Expected: all tests in the module pass, including `a_404_from_the_user_settings_service_is_not_an_error` and the pre-existing `try_get_with_never_caches_an_error`.

- [ ] **Step 6: Full verification and commit**

Run, in order:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Expected: all three succeed with no warnings/errors.

```bash
git add book_bot/src/config.rs \
        book_bot/src/bots/approved_bot/services/user_settings/mod.rs
git commit -m "fix: stop logging a brand-new user's 404 as an error in get_user_settings"
```

---

## Task 5: `batch_downloader` — `Url` config, shared client/status handling, delete dead code

Fixes 8.6 and folds `batch_downloader` into 8.2/8.4. Depends on Task 1.

**Files:**
- Modify: `book_bot/src/config.rs`
- Modify: `book_bot/src/bots/approved_bot/services/batch_downloader.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/download/mod.rs`

**Interfaces:**
- Consumes: `build_url`, `check_response`, `HTTP_CLIENT` from `crate::bots::approved_bot::services` (Task 1).
- Produces: no change to `batch_downloader`'s public function names/signatures.

- [ ] **Step 1: Convert `batch_downloader_url` and `public_batch_downloader_url` to `reqwest::Url` in `config.rs`**

In `book_bot/src/config.rs`, change:

```rust
    pub batch_downloader_url: String,
    pub public_batch_downloader_url: String,
    pub batch_downloader_api_key: String,
```

to:

```rust
    pub batch_downloader_url: reqwest::Url,
    pub public_batch_downloader_url: reqwest::Url,
    pub batch_downloader_api_key: String,
```

and change:

```rust
            batch_downloader_url: get_env("BATCH_DOWNLOADER_URL"),
            public_batch_downloader_url: get_env("PUBLIC_BATCH_DOWNLOADER_URL"),
            batch_downloader_api_key: get_env("BATCH_DOWNLOADER_API_KEY"),
```

to:

```rust
            batch_downloader_url: reqwest::Url::parse(&get_env("BATCH_DOWNLOADER_URL"))
                .unwrap_or_else(|_| panic!("Cannot parse url from BATCH_DOWNLOADER_URL env variable")),
            public_batch_downloader_url: reqwest::Url::parse(&get_env(
                "PUBLIC_BATCH_DOWNLOADER_URL",
            ))
            .unwrap_or_else(|_| {
                panic!("Cannot parse url from PUBLIC_BATCH_DOWNLOADER_URL env variable")
            }),
            batch_downloader_api_key: get_env("BATCH_DOWNLOADER_API_KEY"),
```

- [ ] **Step 2: Rewrite `batch_downloader.rs`**

Replace the entire contents of `book_bot/src/bots/approved_bot/services/batch_downloader.rs` with:

```rust
use smallvec::SmallVec;
use smartstring::alias::String as SmartString;

use serde::{Deserialize, Serialize};

use crate::{
    bots::approved_bot::services::{build_url, check_response, HTTP_CLIENT},
    config,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskObjectType {
    Sequence,
    Author,
    Translator,
}

#[derive(Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    InProgress,
    Archiving,
    Complete,
    Failed,
}

#[derive(Serialize)]
pub struct CreateTaskData {
    pub object_id: u32,
    pub object_type: TaskObjectType,
    pub file_format: String,
    pub allowed_langs: SmallVec<[SmartString; 3]>,
    /// When `true`, archive members have transliterated (GOST 7.79B) names.
    /// Set to `false` to keep Cyrillic names. Mirrors the cache server's
    /// `?normalized=` parameter.
    pub normalized: bool,
}

#[derive(Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub status: TaskStatus,
    pub status_description: String,
    pub error_message: Option<String>,
    pub result_filename: Option<String>,
    pub content_size: Option<u64>,
}

pub async fn create_task(data: CreateTaskData, user_id: Option<u64>) -> anyhow::Result<Task> {
    let url = build_url(&config::CONFIG.batch_downloader_url, ["api", ""])?;

    let mut request = HTTP_CLIENT
        .post(url)
        .json(&data)
        .header("Authorization", &config::CONFIG.batch_downloader_api_key);

    if let Some(uid) = user_id {
        request = request.header("X-User-Id", uid.to_string());
    }

    let response = request.send().await?;

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("batch-downloader service returned an empty response"))
}

pub async fn get_task(task_id: &str) -> anyhow::Result<Task> {
    let url = build_url(
        &config::CONFIG.batch_downloader_url,
        ["api", "check_archive", task_id],
    )?;

    let response = HTTP_CLIENT
        .get(url)
        .header("Authorization", &config::CONFIG.batch_downloader_api_key)
        .send()
        .await?;

    check_response(response, &[])
        .await?
        .ok_or_else(|| anyhow::anyhow!("batch-downloader service returned an empty response"))
}
```

This deletes the local `pub static CLIENT`, the `default_normalized_true` function, and the `#[serde(default = "default_normalized_true")]` attribute on `CreateTaskData::normalized` (dead on a `Serialize`-only struct — fixes 8.6). `create_task` already sent the body via `.json(&data)`, so that part is unchanged.

- [ ] **Step 3: Update the two link-building call sites in `download/mod.rs`**

In `book_bot/src/bots/approved_bot/modules/download/mod.rs`, add `build_url` to the `services::{...}` import block. Change:

```rust
                donation_notifications::send_donation_notification,
                user_settings::{
                    get_user_file_name_lang_for, get_user_or_default_lang_codes, FileNameLang,
                },
            },
```

to:

```rust
                donation_notifications::send_donation_notification,
                user_settings::{
                    get_user_file_name_lang_for, get_user_or_default_lang_codes, FileNameLang,
                },
                build_url,
            },
```

In `send_archive_link`, change:

```rust
    let link = format!(
        "{}/api/download/{}",
        config::CONFIG.public_batch_downloader_url,
        task.id
    );
```

to:

```rust
    let link = build_url(
        &config::CONFIG.public_batch_downloader_url,
        ["api", "download", &task.id],
    )?
    .to_string();
```

In `wait_archive`, change:

```rust
    let link = format!(
        "{}/api/download/{}",
        config::CONFIG.batch_downloader_url,
        task.id
    );
```

to:

```rust
    let link = build_url(
        &config::CONFIG.batch_downloader_url,
        ["api", "download", &task.id],
    )?
    .to_string();
```

`download_file_by_link` (called just below with `link`) keeps its existing `link: String` parameter — no signature change needed there.

- [ ] **Step 4: Build and run the full test suite**

Run: `cargo build --workspace && cargo test --workspace`
Expected: builds cleanly; all existing tests pass (there are no pre-existing tests in `batch_downloader.rs` or the touched parts of `download/mod.rs` to regress).

- [ ] **Step 5: Full verification and commit**

Run, in order:
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Expected: all three succeed with no warnings/errors.

```bash
git add book_bot/src/config.rs \
        book_bot/src/bots/approved_bot/services/batch_downloader.rs \
        book_bot/src/bots/approved_bot/modules/download/mod.rs
git commit -m "refactor: unify batch_downloader onto shared HTTP client/URL helpers, drop dead serde default"
```

---

## Task 6: Final acceptance-criteria verification

No code changes — confirms the spec's acceptance criteria hold once Tasks 1-5 are all merged.

**Files:** none modified.

- [ ] **Step 1: Confirm there is exactly one `reqwest::Client::builder` in the codebase**

Run: `grep -rn "reqwest::Client::builder" book_bot/src`
Expected: exactly one match, in `book_bot/src/bots/approved_bot/services/mod.rs` (the `bots_manager::bot_manager_client::CLIENT` static is out of scope for this plan per the Global Constraints note — if this grep surfaces it too, that's expected and not a regression to fix here).

- [ ] **Step 2: Confirm `encode_path_segment` and `build_cache_url` no longer exist**

Run: `grep -rn "encode_path_segment\|build_cache_url" book_bot/src`
Expected: no matches.

- [ ] **Step 3: Confirm no service base URL is still a bare `String` being concatenated with `format!`**

Run: `grep -rn 'format!("{}{}"' book_bot/src/bots/approved_bot/services`
Expected: no matches.

- [ ] **Step 4: Full workspace verification**

Run, in order:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Expected: all three succeed with no warnings/errors — this is the exact CI gate from `.github/workflows/ci.yml`.
