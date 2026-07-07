# Security: Webhook Secret Token & Token Log Masking — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent full bot tokens from appearing in logs/traces, and authenticate incoming webhook updates with a `X-Telegram-Bot-Api-Secret-Token` header.

**Architecture:** Add a `mask_token` helper used everywhere tokens currently appear in logs; write a custom TraceLayer `MakeSpan` that replaces the token path segment with the public bot-id only; add `WEBHOOK_SECRET_TOKEN` to config, pass it via `SetWebhook::secret_token`, and validate it in the axum handler before any processing.

**Tech Stack:** Rust, axum 0.8, tower-http 0.6, teloxide 0.17 (teloxide-core 0.13), metrics 0.24.

## Global Constraints

- No full bot token may appear in any log line or tracing span at any level.
- `X-Telegram-Bot-Api-Secret-Token` must be validated before the request reaches any bot dispatch logic.
- `WEBHOOK_SECRET_TOKEN` env var is mandatory — startup panics if absent (consistent with every other env var in `config.rs`).
- Use `mask_token` (defined in Task 1) consistently — do not inline the masking logic again.
- metrics counter name: `"webhook_secret_rejected_total"`.

---

### Task 1: Add `mask_token` and path-masking helpers to `bots_manager/utils.rs`

**Files:**
- Modify: `book_bot/src/bots_manager/utils.rs`

**Interfaces:**
- Produces:
  - `pub fn mask_token(token: &str) -> String` — returns first 8 chars of `token` + `"…"`
  - `pub fn mask_uri_path(path: &str) -> String` — replaces a Telegram-token path segment with `"/[bot:{id}]/"`, leaves other paths unchanged

- [ ] **Step 1: Write failing tests**

Add to `book_bot/src/bots_manager/utils.rs` (below the existing `tuple_first_mut` function):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_token_long() {
        assert_eq!(mask_token("123456789:ABCDEFGHIJK-long-secret"), "12345678…");
    }

    #[test]
    fn mask_token_exactly_8() {
        assert_eq!(mask_token("12345678"), "12345678…");
    }

    #[test]
    fn mask_token_short() {
        assert_eq!(mask_token("abc"), "abc…");
    }

    #[test]
    fn mask_uri_path_telegram_token() {
        assert_eq!(mask_uri_path("/987654321:XYZ-secret/"), "/[bot:987654321]/");
    }

    #[test]
    fn mask_uri_path_no_token() {
        assert_eq!(mask_uri_path("/metrics"), "/metrics");
        assert_eq!(mask_uri_path("/health"), "/health");
    }
}
```

- [ ] **Step 2: Run to verify tests fail**

```bash
cargo test -p book_bot mask_token 2>&1 | tail -20
cargo test -p book_bot mask_uri_path 2>&1 | tail -20
```

Expected: `error[E0425]: cannot find function` (functions not yet defined).

- [ ] **Step 3: Implement the helpers**

Add below `tuple_first_mut` in `book_bot/src/bots_manager/utils.rs`:

```rust
pub fn mask_token(token: &str) -> String {
    format!("{}…", &token[..token.len().min(8)])
}

pub fn mask_uri_path(path: &str) -> String {
    let stripped = path.trim_start_matches('/');
    let end = stripped.find('/').unwrap_or(stripped.len());
    let segment = &stripped[..end];

    if let Some(colon) = segment.find(':') {
        let bot_id = &segment[..colon];
        if !bot_id.is_empty() && bot_id.chars().all(|c| c.is_ascii_digit()) {
            return format!("/[bot:{}]/", bot_id);
        }
    }

    path.to_string()
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p book_bot mask_token 2>&1 | tail -20
cargo test -p book_bot mask_uri_path 2>&1 | tail -20
```

Expected: all 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add book_bot/src/bots_manager/utils.rs
git commit -m "feat(security): add mask_token and mask_uri_path helpers"
```

---

### Task 2: Use `mask_token` in all existing log statements

**Files:**
- Modify: `book_bot/src/bots_manager/mod.rs` (line 77)
- Modify: `book_bot/src/bots_manager/axum_server.rs` (line 59)

**Interfaces:**
- Consumes: `mask_token` from `super::utils` (Task 1)

- [ ] **Step 1: Fix eviction listener in `mod.rs`**

In `book_bot/src/bots_manager/mod.rs`, add the `utils` import at the top of the `BOTS_ROUTES` eviction listener block. The eviction listener closure is at lines 76–83. Replace:

```rust
        .eviction_listener(|token, value: StopTokenWithSender, _cause| {
            log::info!("Stop Bot(token={token})!");
```

with:

```rust
        .eviction_listener(|token, value: StopTokenWithSender, _cause| {
            log::info!("Stop Bot(token={})!", crate::bots_manager::utils::mask_token(&token));
```

- [ ] **Step 2: Fix error log in `axum_server.rs`**

In `book_bot/src/bots_manager/axum_server.rs`, add import at the top of the file (after the existing `use` statements):

```rust
use crate::bots_manager::utils::mask_token;
```

Then replace lines 58–61:

```rust
                        log::error!(
                            "Cannot get a bot with token: {}...",
                            &token[..token.len().min(5)]
                        );
```

with:

```rust
                        log::error!(
                            "Cannot get a bot with token: {}",
                            mask_token(&token)
                        );
```

- [ ] **Step 3: Build to verify it compiles**

```bash
cargo build -p book_bot 2>&1 | tail -20
```

Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```bash
git add book_bot/src/bots_manager/mod.rs book_bot/src/bots_manager/axum_server.rs
git commit -m "fix(security): mask bot token in eviction and error logs"
```

---

### Task 3: Custom TraceLayer span that masks the token path segment

**Files:**
- Modify: `book_bot/src/bots_manager/axum_server.rs`

**Interfaces:**
- Consumes: `mask_uri_path` from `crate::bots_manager::utils` (Task 1)
- Produces: `BotIdMakeSpan` struct used in `TraceLayer::make_span_with`

- [ ] **Step 1: Add `BotIdMakeSpan` struct and import**

In `book_bot/src/bots_manager/axum_server.rs`, add to the existing `use crate::bots_manager::utils::mask_token;` line (extend it):

```rust
use crate::bots_manager::utils::{mask_token, mask_uri_path};
```

Then add this struct and impl anywhere before `start_axum_server`:

```rust
struct BotIdMakeSpan;

impl<B> tower_http::trace::MakeSpan<B> for BotIdMakeSpan {
    fn make_span(&mut self, request: &axum::http::Request<B>) -> tracing::Span {
        let masked = mask_uri_path(request.uri().path());
        tracing::info_span!(
            "request",
            method = %request.method(),
            uri = %masked,
            version = ?request.version(),
        )
    }
}
```

- [ ] **Step 2: Replace `DefaultMakeSpan` with `BotIdMakeSpan` in TraceLayer**

In `start_axum_server`, replace:

```rust
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        );
```

with:

```rust
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(BotIdMakeSpan)
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
        );
```

The `Level` import is no longer needed for `make_span_with`; leave it in case it's still used elsewhere, or remove if it produces a dead-code warning.

- [ ] **Step 3: Build to verify it compiles**

```bash
cargo build -p book_bot 2>&1 | tail -20
```

Expected: `Finished` with no errors. If there's an unused import warning for `Level`, remove it from the `use tracing::Level;` line.

- [ ] **Step 4: Commit**

```bash
git add book_bot/src/bots_manager/axum_server.rs
git commit -m "fix(security): replace DefaultMakeSpan with bot-id-only span in TraceLayer"
```

---

### Task 4: Add `WEBHOOK_SECRET_TOKEN` to config

**Files:**
- Modify: `book_bot/src/config.rs`

**Interfaces:**
- Produces: `config::CONFIG.webhook_secret_token: String`

- [ ] **Step 1: Add field to `Config` struct**

In `book_bot/src/config.rs`, add `webhook_secret_token` after `webhook_port`:

```rust
pub struct Config {
    pub telegram_bot_api: reqwest::Url,

    pub webhook_base_url: String,
    pub webhook_port: u16,
    pub webhook_secret_token: String,
    // ... rest unchanged
```

- [ ] **Step 2: Load `WEBHOOK_SECRET_TOKEN` in `Config::load`**

In `Config::load()`, add after `webhook_port`:

```rust
            webhook_port: get_env("WEBHOOK_PORT").parse().unwrap(),
            webhook_secret_token: get_env("WEBHOOK_SECRET_TOKEN"),
```

- [ ] **Step 3: Build to verify it compiles**

```bash
cargo build -p book_bot 2>&1 | tail -20
```

Expected: `Finished` (will panic at runtime if env var absent — that is intentional, consistent with every other required env var in this codebase).

- [ ] **Step 4: Commit**

```bash
git add book_bot/src/config.rs
git commit -m "feat(config): add WEBHOOK_SECRET_TOKEN env variable"
```

---

### Task 5: Pass `secret_token` when registering webhooks

**Files:**
- Modify: `book_bot/src/bots_manager/internal.rs` (line 74)

**Interfaces:**
- Consumes: `config::CONFIG.webhook_secret_token: String` (Task 4)
- Telegram constraint: `secret_token` must be 1–256 chars, only `A-Z a-z 0-9 _ -`.

- [ ] **Step 1: Chain `.secret_token()` on `set_webhook` call**

In `book_bot/src/bots_manager/internal.rs`, replace line 74:

```rust
        match bot.set_webhook(url.clone()).await {
```

with:

```rust
        match bot
            .set_webhook(url.clone())
            .secret_token(config::CONFIG.webhook_secret_token.clone())
            .await
        {
```

- [ ] **Step 2: Build to verify it compiles**

```bash
cargo build -p book_bot 2>&1 | tail -20
```

Expected: `Finished` with no errors.

- [ ] **Step 3: Commit**

```bash
git add book_bot/src/bots_manager/internal.rs
git commit -m "feat(security): pass WEBHOOK_SECRET_TOKEN when registering webhooks"
```

---

### Task 6: Validate `X-Telegram-Bot-Api-Secret-Token` header in axum handler

**Files:**
- Modify: `book_bot/src/bots_manager/axum_server.rs`

**Interfaces:**
- Consumes: `config::CONFIG.webhook_secret_token: String` (Task 4)
- Consumes: `metrics::counter!` from the `metrics` crate (already in `Cargo.toml` as `metrics = "0.24.2"`)

- [ ] **Step 1: Add `HeaderMap` import**

In `book_bot/src/bots_manager/axum_server.rs`, add to the existing axum imports:

```rust
use axum::http::HeaderMap;
```

- [ ] **Step 2: Add `headers` parameter to `telegram_request`**

Change the function signature from:

```rust
    async fn telegram_request(
        State(start_bot_mutex): State<Arc<Mutex<()>>>,
        Path(token): Path<String>,
        input: String,
    ) -> impl IntoResponse {
```

to:

```rust
    async fn telegram_request(
        State(start_bot_mutex): State<Arc<Mutex<()>>>,
        Path(token): Path<String>,
        headers: HeaderMap,
        input: String,
    ) -> impl IntoResponse {
```

- [ ] **Step 3: Add secret-token validation at the top of the handler body**

Insert immediately after the opening brace of `telegram_request`, before any other logic:

```rust
        let expected_secret = config::CONFIG.webhook_secret_token.as_str();
        let provided_secret = headers
            .get("x-telegram-bot-api-secret-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided_secret != expected_secret {
            metrics::counter!("webhook_secret_rejected_total").increment(1u64);
            return StatusCode::FORBIDDEN;
        }
```

- [ ] **Step 4: Build to verify it compiles**

```bash
cargo build -p book_bot 2>&1 | tail -20
```

Expected: `Finished` with no errors.

- [ ] **Step 5: Verify rejection behaviour manually**

Start the server locally with `WEBHOOK_SECRET_TOKEN=test-secret-abc` and all other required env vars set. Then:

```bash
# Should get 403 (missing header)
curl -s -o /dev/null -w "%{http_code}" -X POST http://localhost:$WEBHOOK_PORT/fake-token/
# Expected: 403

# Should get 404 or 503 (correct secret, unknown token — past the auth gate)
curl -s -o /dev/null -w "%{http_code}" -X POST \
  -H "X-Telegram-Bot-Api-Secret-Token: test-secret-abc" \
  -H "Content-Type: application/json" \
  -d '{}' \
  http://localhost:$WEBHOOK_PORT/fake-token/
# Expected: 404
```

Also grep the logs to confirm no full token appears:

```bash
# Token-shaped strings should NOT appear in logs — only masked forms like "12345678…" or "[bot:12345]"
grep -E "[0-9]{5,}:[A-Za-z0-9_-]{20,}" /path/to/log-output && echo "FAIL: token leaked" || echo "OK: no token in logs"
```

- [ ] **Step 6: Commit**

```bash
git add book_bot/src/bots_manager/axum_server.rs
git commit -m "feat(security): validate X-Telegram-Bot-Api-Secret-Token header, reject with 403 + metric"
```

---

## Self-Review Against Spec

| Spec requirement | Covered by |
|---|---|
| `mask_token` helper in `utils.rs` | Task 1 |
| Eviction listener no longer logs full token | Task 2 |
| `TraceLayer` URI no longer contains full token | Task 3 |
| Existing `&token[..5]` usage in axum_server replaced with `mask_token` | Task 2 |
| `WEBHOOK_SECRET_TOKEN` env variable | Task 4 |
| `SetWebhook::secret_token` set on registration | Task 5 |
| 403 on missing/invalid `X-Telegram-Bot-Api-Secret-Token` | Task 6 |
| Metric for rejected requests | Task 6 |
| Legitimate updates still processed | Task 6 step 5 (manual verification) |
| Webhooks re-set on startup picks up new secret | inherent — `check_uninited` runs on startup and calls `set_webhook` |
