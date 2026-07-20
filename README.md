# book_bot

A Telegram bot that serves book search, download, and annotation lookups, backed by a set of internal microservices.

## Architecture

- **`bots_manager`** (`book_bot/src/bots_manager`) — polls a manager service (`MANAGER_URL`) for the list of approved bot tokens and spins up one Telegram long-polling bot instance per token.
- **Approved bot** (`book_bot/src/bots/approved_bot`) — the actual command/callback handlers (search, download, annotations, settings, update history, random).
- **Webhook server** (`axum`, wired in `main.rs`) — exposes a `/health` endpoint (used by the Docker `HEALTHCHECK`) and Prometheus metrics via `axum-prometheus`.
- **External services** the bot talks to over HTTP, each with its own base URL + API key: a book manager/registration service, a user-settings service, a book-library/annotations service, a cache service, and a batch-downloader service. See the env table below.
- Errors are tracked via Sentry (`sentry` + `sentry-tracing`) when `SENTRY_DSN` is set; logs go through `tracing`, filtered by `RUST_LOG`.

## Configuration

All variables below are read once at startup by `Config::load()` in `book_bot/src/config.rs`. Missing a required one panics at startup with `Cannot get the <NAME> env variable`.

| Variable | Required | Purpose |
|---|---|---|
| `TELEGRAM_BOT_API_ROOT` | yes | Base URL of the Telegram Bot API server (local `telegram-bot-api` instance or `https://api.telegram.org`) |
| `WEBHOOK_BASE_URL` | yes | Public base URL Telegram/webhook clients use to reach this service |
| `WEBHOOK_PORT` | yes | Port the webhook/health/metrics server binds to |
| `WEBHOOK_SECRET_TOKEN` | yes | Secret token used to validate incoming webhook requests |
| `MANAGER_URL` | yes | Base URL of the bots-manager service (source of approved bot tokens) |
| `MANAGER_API_KEY` | yes | API key for `MANAGER_URL` |
| `USER_SETTINGS_URL` | yes | Base URL of the user-settings service (scheme+host+port only, no path — see the comment in `config.rs`) |
| `USER_SETTINGS_API_KEY` | yes | API key for `USER_SETTINGS_URL` |
| `BOOK_SERVER_URL` | yes | Base URL of the book-library/annotations service |
| `BOOK_SERVER_API_KEY` | yes | API key for `BOOK_SERVER_URL` |
| `CACHE_SERVER_URL` | yes | Base URL of the channel-cache service |
| `CACHE_SERVER_API_KEY` | yes | API key for `CACHE_SERVER_URL` |
| `BATCH_DOWNLOADER_URL` | yes | Internal base URL of the batch-downloader service |
| `PUBLIC_BATCH_DOWNLOADER_URL` | yes | Publicly reachable base URL of the batch-downloader service |
| `BATCH_DOWNLOADER_API_KEY` | yes | API key for the batch-downloader service |
| `SENTRY_DSN` | no | Sentry DSN; error reporting is skipped entirely if unset |
| `RUST_LOG` | no | `tracing`/`EnvFilter` directive (e.g. `debug,tower_http=warn`); defaults to `info` |

`test_env/dev.env` has a working example set of values for local development (against the mock services described below), and `test_env/db.json` shows the shape the manager service returns (`token`, `status`, `cache` per bot).

## Running locally

1. Start the local dependencies (a local Telegram Bot API server + a mock manager service):
   ```bash
   cd test_env
   docker compose up
   ```
2. Load the example env and run the bot:
   ```bash
   source test_env/dev.env
   cargo run --bin book_bot
   ```

## Running in Docker

The production image is built from `docker/build.dockerfile` (multi-stage: `cargo-chef` dependency caching, then a `debian:bookworm-slim` runtime image running as a non-root `app` user):

```bash
docker build -f docker/build.dockerfile -t book_bot .
docker run --env-file test_env/dev.env -p 8080:8080 book_bot
```

The container's `HEALTHCHECK` polls `http://localhost:${WEBHOOK_PORT}/health`.

## Tests

```bash
cargo test --workspace
```

## Dependency hygiene

- `Cargo.lock` currently pins duplicate major versions of a few transitive crates (e.g. two `http` majors pulled in by `axum`/`reqwest`/`teloxide`, three `rand` majors, multiple `windows-sys` majors). This is expected in a 380+ package dependency tree with several independently-versioned ecosystems (axum vs. teloxide vs. reqwest) and isn't independently fixable without upstream changes.
- Periodically run `cargo update` and `cargo tree -d` to check whether any duplication has become prunable, and `cargo tree -i <crate>@<version>` to find which direct dependency pulls in a specific duplicate.
