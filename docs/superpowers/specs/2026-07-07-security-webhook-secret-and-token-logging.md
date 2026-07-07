# Spec 01: Webhook security — secret_token validation and bot token leakage in logs

- **Priority:** high
- **Effort:** M
- **Category:** security

## Problem

### 1.1. Full bot tokens leak into logs

- `book_bot/src/bots_manager/mod.rs:77` — the `BOTS_ROUTES` eviction listener logs the full token at INFO level:
  ```rust
  log::info!("Stop Bot(token={token})!");
  ```
- The webhook route is `/{token}/` (`book_bot/src/bots_manager/axum_server.rs:111`), and `TraceLayer` with `DefaultMakeSpan::new().level(Level::INFO)` logs the URI of every incoming request — i.e. every bot's full token ends up in logs on every update, and potentially in Sentry breadcrumbs via tracing.
- The code already shows awareness of the problem — in `axum_server.rs:58-61` the token is masked (`&token[..token.len().min(5)]`) — but only in that single place.

### 1.2. Webhook updates are not authenticated

- `book_bot/src/bots_manager/internal.rs:74` — `bot.set_webhook(url.clone())` is called without a `secret_token`.
- The axum handler (`axum_server.rs:31-104`) does not check the `X-Telegram-Bot-Api-Secret-Token` header.
- The only "authentication" is knowledge of the bot token in the URL path. Combined with 1.1 (tokens in logs), anyone with access to logs/traces can forge updates on behalf of Telegram.

## Proposed solution

1. **Mask tokens everywhere:**
   - Add a helper `fn mask_token(token: &str) -> String` (first 5–8 chars + `…`) in `bots_manager/utils.rs` and use it in all log statements, including the eviction listener.
   - For `TraceLayer`, write a custom `make_span_with` that replaces the path with a masked one (or log only the `bot_id` — the leading numeric part of the token, which is not secret).
2. **Secret token for webhooks:**
   - Generate a secret (a shared one from a `WEBHOOK_SECRET_TOKEN` env variable, or per-bot derived from the token via HMAC).
   - Pass it via `SetWebhook::secret_token` when setting the webhook.
   - In the axum handler, reject requests with a missing/invalid `X-Telegram-Bot-Api-Secret-Token` header (403), with a metric counting rejected requests.

## Acceptance criteria

- No full bot token appears anywhere in logs (including tower_http trace spans) — verified by grepping the logs of a local run with a test bot.
- A request to the webhook URL without the correct `X-Telegram-Bot-Api-Secret-Token` gets 403 and never reaches the dispatcher.
- Legitimate Telegram updates are processed as before (after webhooks are re-set with the secret).

## Risks / notes

- Rollout requires re-setting webhooks for all bots (this already happens automatically on startup — `check_bots`).
- If the secret is per-bot, store it next to the `BOTS_DATA` entry so the handler can validate without recomputation.
