# Dependency Cleanup and Project Hygiene Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close out spec `docs/specs/13-dependencies-and-project-hygiene.md` — add a root README, replace hand-written `Display`/`Error` impls with `thiserror`, drop the redundant `dateparser` dependency, collapse `strum`/`strum_macros` into one dependency declaration.

**Architecture:** Four independent, sequential edits to the `book_bot` crate (Cargo.toml + three source files) plus one new root-level documentation file. No behavior changes — every task preserves existing external behavior and is covered by the crate's existing `#[cfg(test)]` unit tests.

**Tech Stack:** Rust 2021, Cargo workspace (`book_bot` binary crate + `book_bot_macros` proc-macro crate), `thiserror` (new dependency), `chrono`, `strum`.

## Global Constraints

- Workspace root is `/Users/kurbezz/Projects/books_project/book_bot`; the binary crate lives in `book_bot/` (its `Cargo.toml` is `book_bot/Cargo.toml`).
- All commands below assume cwd = workspace root unless stated otherwise.
- Build and test must stay green after every task: `cargo build --workspace` and `cargo test --workspace`.
- Do not change the `panic = "abort"` setting or its comment in the root `Cargo.toml` — spec item 13.6 is already satisfied (see "Already satisfied" note below).
- No Co-Authored-By trailer on commits in this repo (existing project convention).

## Already satisfied — no task needed

- **13.6 (`panic = "abort"` decision):** the root `Cargo.toml` (lines 17-20) already carries a comment recording the decision and its rationale (container orchestrator restarts on exit). Nothing to do.
- **13.5 (duplicate transitive dependencies):** not independently fixable and not part of the acceptance criteria — the spec itself says "not directly fixable, but worth diagnosing." Task 5 folds the diagnosis into the new README as a documented finding rather than a standalone code task.

---

### Task 1: Add `thiserror` and convert the two generic parse errors

**Files:**
- Modify: `book_bot/Cargo.toml`
- Modify: `book_bot/src/bots/approved_bot/modules/utils/errors.rs`

**Interfaces:**
- Produces: `CallbackQueryParseError` and `CommandParseError` keep their existing names, `Debug` derive, and zero-field unit-struct shape (all 20+ call sites across `annotations/`, `book/`, `download/`, `utils/filter_command.rs` construct them as bare `CallbackQueryParseError` / `CommandParseError` with no fields — this must keep compiling unchanged).

- [ ] **Step 1: Add the dependency**

Run from the workspace root:
```bash
cd book_bot && cargo add thiserror
```
This adds a `thiserror = "..."` line under `[dependencies]` in `book_bot/Cargo.toml` at the latest compatible version and updates the root `Cargo.lock`.

- [ ] **Step 2: Write the failing test**

Add to the bottom of `book_bot/src/bots/approved_bot/modules/utils/errors.rs` (the file currently has no `#[cfg(test)]` block):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_query_parse_error_has_readable_message() {
        assert_eq!(
            CallbackQueryParseError.to_string(),
            "failed to parse callback query data"
        );
    }

    #[test]
    fn command_parse_error_has_readable_message() {
        assert_eq!(
            CommandParseError.to_string(),
            "failed to parse command"
        );
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd book_bot && cargo test --lib utils::errors::tests -- --nocapture`
Expected: FAIL — current `Display` impl prints the Debug form (`"CallbackQueryParseError"` / `"CommandParseError"`), not the new sentence, so both assertions fail.

- [ ] **Step 4: Replace the hand-written impls with `thiserror`**

Replace the full contents of `book_bot/src/bots/approved_bot/modules/utils/errors.rs` (keeping the test module you just added at the bottom) with:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
#[error("failed to parse callback query data")]
pub struct CallbackQueryParseError;

#[derive(Debug, Error)]
#[error("failed to parse command")]
pub struct CommandParseError;
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd book_bot && cargo test --lib utils::errors::tests`
Expected: PASS (2 passed)

- [ ] **Step 6: Build the whole workspace to confirm no call site broke**

Run: `cargo build --workspace`
Expected: clean build, no errors (all existing `CallbackQueryParseError` / `CommandParseError` construction sites are unit-struct literals and are unaffected by the derive change).

- [ ] **Step 7: Commit**

```bash
git add book_bot/Cargo.toml Cargo.lock book_bot/src/bots/approved_bot/modules/utils/errors.rs
git commit -m "refactor: derive thiserror::Error for CallbackQueryParseError/CommandParseError"
```

---

### Task 2: Convert `AnnotationFormatError` to `thiserror`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/annotations/errors.rs`
- Modify: `book_bot/src/bots/approved_bot/modules/annotations/mod.rs:101-107` (the only construction site)

**Interfaces:**
- Consumes: `thiserror::Error` (added in Task 1), `AnnotationCommand` from `book_bot/src/bots/approved_bot/modules/annotations/commands.rs` (already `#[derive(Debug, Clone)]`, unchanged).
- Produces: `AnnotationFormatError { pub command: AnnotationCommand, pub text: String }` — field names drop the current `_` prefix (the fields become genuinely used inside the `#[error(...)]` message, so the underscore-to-suppress-dead-code-warning convention no longer applies).

- [ ] **Step 1: Write the failing test**

Add to the bottom of `book_bot/src/bots/approved_bot/modules/annotations/errors.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::approved_bot::modules::annotations::commands::AnnotationCommand;

    #[test]
    fn message_includes_command_and_text() {
        let err = AnnotationFormatError {
            command: AnnotationCommand::Book { id: 42 },
            text: "   \n  ".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Book"));
        assert!(msg.contains("42"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd book_bot && cargo test --lib annotations::errors::tests -- --nocapture`
Expected: FAIL to compile — the struct still has fields `_command`/`_text`, not `command`/`text`.

- [ ] **Step 3: Replace the hand-written impl with `thiserror`**

Replace the top of `book_bot/src/bots/approved_bot/modules/annotations/errors.rs` (everything above the test module added in Step 1) with:

```rust
use thiserror::Error;

use super::commands::AnnotationCommand;

#[derive(Debug, Error)]
#[error("annotation text for {command:?} is not normal text: {text:?}")]
pub struct AnnotationFormatError {
    pub command: AnnotationCommand,
    pub text: String,
}
```

- [ ] **Step 4: Update the construction site**

In `book_bot/src/bots/approved_bot/modules/annotations/mod.rs`, the `send_annotation_handler` function currently builds the error like this (around line 101-107):

```rust
    if !annotation.is_normal_text() {
        return Err(AnnotationFormatError {
            _command: command,
            _text: annotation.get_text().to_string(),
        }
        .into());
    }
```

Change the field names to match the renamed struct fields:

```rust
    if !annotation.is_normal_text() {
        return Err(AnnotationFormatError {
            command,
            text: annotation.get_text().to_string(),
        }
        .into());
    }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd book_bot && cargo test --lib annotations::errors::tests`
Expected: PASS (1 passed)

- [ ] **Step 6: Build the whole workspace**

Run: `cargo build --workspace`
Expected: clean build, no errors.

- [ ] **Step 7: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/annotations/errors.rs book_bot/src/bots/approved_bot/modules/annotations/mod.rs
git commit -m "refactor: derive thiserror::Error for AnnotationFormatError"
```

---

### Task 3: Remove `dateparser`, parse dates with `chrono::NaiveDate` directly

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/update_history/callback_data.rs:1-36`
- Modify: `book_bot/Cargo.toml` (remove the `dateparser` line)

**Interfaces:**
- No public interface changes — `UpdateLogCallbackData::from_str` keeps its existing signature and `Err = strum::ParseError` type. Existing tests in the same file (`round_trip`, `page_zero_normalized_to_one`, `rejects_garbage`, `rejects_invalid_date`) must keep passing unmodified — they are the regression check for this task, so no new test is added.

- [ ] **Step 1: Confirm existing tests pass before touching anything**

Run: `cd book_bot && cargo test --lib update_history::callback_data::tests`
Expected: PASS (4 passed) — this is the baseline you must not break.

- [ ] **Step 2: Replace `dateparser` usage with `NaiveDate::parse_from_str`**

In `book_bot/src/bots/approved_bot/modules/update_history/callback_data.rs`, remove the import on line 4:

```rust
use dateparser::parse;
```

And replace the parsing lines (currently lines 28-33):

```rust
        let from: NaiveDate = parse(&caps["from"])
            .map_err(|_| strum::ParseError::VariantNotFound)?
            .date_naive();
        let to: NaiveDate = parse(&caps["to"])
            .map_err(|_| strum::ParseError::VariantNotFound)?
            .date_naive();
```

with:

```rust
        let from = NaiveDate::parse_from_str(&caps["from"], "%Y-%m-%d")
            .map_err(|_| strum::ParseError::VariantNotFound)?;
        let to = NaiveDate::parse_from_str(&caps["to"], "%Y-%m-%d")
            .map_err(|_| strum::ParseError::VariantNotFound)?;
```

- [ ] **Step 3: Remove the dependency from Cargo.toml**

In `book_bot/Cargo.toml`, delete this line (currently line 57):

```toml
dateparser = "0.2.1"
```

- [ ] **Step 4: Run the existing tests to verify they still pass**

Run: `cd book_bot && cargo test --lib update_history::callback_data::tests`
Expected: PASS (4 passed) — same four tests as Step 1, now exercising `NaiveDate::parse_from_str`.

- [ ] **Step 5: Update the lockfile and confirm the transitive tree shrank**

Run: `cargo build --workspace` (from the workspace root; this regenerates `Cargo.lock` to drop `dateparser` and its transitive deps).
Then run: `grep -c '^name = "dateparser"' Cargo.lock`
Expected: `0` (no match — `grep -c` with no match still exits non-zero, that's fine, the count printed is `0`).

- [ ] **Step 6: Commit**

```bash
git add book_bot/Cargo.toml Cargo.lock book_bot/src/bots/approved_bot/modules/update_history/callback_data.rs
git commit -m "refactor: parse update-log dates with chrono directly, drop dateparser"
```

---

### Task 4: Collapse `strum` + `strum_macros` into one dependency

**Files:**
- Modify: `book_bot/Cargo.toml`

**Interfaces:**
- No source changes — confirmed via `grep -rn "strum_macros::" book_bot/src/` and `grep -rn "use strum" book_bot/src/` that no file in `book_bot/src/` imports or references `strum_macros` directly; all current usage is `strum::ParseError` / `strum::ParseError::VariantNotFound`, which is provided by the `strum` crate itself regardless of the `derive` feature.

- [ ] **Step 1: Re-confirm no direct `strum_macros` usage (safety check before editing)**

Run: `grep -rn "strum_macros" book_bot/src/`
Expected: no output (already verified during planning; re-run to catch any drift before you edit).

- [ ] **Step 2: Edit Cargo.toml**

In `book_bot/Cargo.toml`, replace these two lines (currently lines 50-51):

```toml
strum = "0.27.1"
strum_macros = "0.27.1"
```

with:

```toml
strum = { version = "0.27", features = ["derive"] }
```

- [ ] **Step 3: Build to confirm nothing broke**

Run: `cargo build --workspace`
Expected: clean build, no errors.

- [ ] **Step 4: Run the full test suite**

Run: `cargo test --workspace`
Expected: all existing tests pass (no behavior touched by this task).

- [ ] **Step 5: Commit**

```bash
git add book_bot/Cargo.toml Cargo.lock
git commit -m "chore: collapse strum_macros into strum's derive feature"
```

---

### Task 5: Add root README

**Files:**
- Create: `README.md` (workspace root, next to `LICENSE.md` and the root `Cargo.toml`)

**Interfaces:**
- None (documentation only). Content is sourced from `book_bot/src/config.rs` (the `get_env` calls in `Config::load`, lines 44-82, plus the optional `SENTRY_DSN` on line 82 and `RUST_LOG` read in `book_bot/src/main.rs:49`) and from `test_env/docker-compose.yml` + `test_env/dev.env` for the local-run story.

- [ ] **Step 1: Write `README.md`**

Create `/Users/kurbezz/Projects/books_project/book_bot/README.md` with this content:

```markdown
# book_bot

A Telegram bot that serves book search, download, and annotation lookups, backed by a set of internal microservices.

## Architecture

- **`bots_manager`** (`book_bot/src/bots_manager`) — polls a manager service (`MANAGER_URL`) for the list of approved bot tokens and spins up one Telegram long-polling bot instance per token.
- **Approved bot** (`book_bot/src/bots/approved_bot`) — the actual command/callback handlers (search, download, annotations, settings, update history, random).
- **Webhook server** (`axum`, in `book_bot/src/bots_manager/axum_server.rs`, started from `main.rs`) — exposes a `/health` endpoint (used by the Docker `HEALTHCHECK`) and Prometheus metrics via `axum-prometheus`.
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

`test_env/` is a **gitignored, developer-local** directory (see `.gitignore`) for local-only secrets and mock-service scaffolding — it is never committed, so it does not exist after a fresh clone. Set it up yourself as described below.

## Running locally

1. Recreate `test_env/docker-compose.yml` with a local Telegram Bot API server and a mock manager service:
   ```yaml
   services:
     telegram_bot_api:
       image: aiogram/telegram-bot-api:latest
       environment:
         TELEGRAM_LOCAL: 1
         TELEGRAM_API_ID: 39920
         TELEGRAM_API_HASH: 0f4dd1c80b30e70e2af60ef61f6ded02
       ports:
         - 8081:8081
         - 8082:8082

     json_server:
       image: clue/json-server
       command: --watch /data/db.json
       ports:
         - 3000:80
       volumes:
         - ./db.json:/data/db.json
   ```
   (`TELEGRAM_API_ID`/`TELEGRAM_API_HASH` above are the standard public test credentials used with a local `telegram-bot-api` instance, not a secret specific to this project.)

2. Add `test_env/db.json`. `json-server` exposes each top-level key of this file as its own route, so the `api` key below is served at `/api` — set `MANAGER_URL=http://localhost:3000/api` accordingly (unlike the other `*_URL` variables, `MANAGER_URL` is used as-is with no path appended, so it must include `/api`):
   ```json
   {
     "api": [
       { "id": 1, "token": "<your test bot token>", "status": "approved", "cache": "no_cache" }
     ]
   }
   ```

3. Start the mocks:
   ```bash
   cd test_env && docker compose up
   ```

4. Create `test_env/dev.env` with `export VAR=value` lines for **every required variable** in the table above (all 15 — a partial file will panic at startup on the first missing one), then load it and run the bot:
   ```bash
   source test_env/dev.env
   cargo run --bin book_bot
   ```

## Running in Docker

The production image is built from `docker/build.dockerfile` (multi-stage: `cargo-chef` dependency caching, then a `debian:bookworm-slim` runtime image running as a non-root `app` user):

```bash
docker build -f docker/build.dockerfile -t book_bot .
docker run --env-file <(sed 's/^export //' test_env/dev.env) -p <WEBHOOK_PORT>:<WEBHOOK_PORT> book_bot
```

Docker's `--env-file` expects plain `KEY=VALUE` lines and does not strip a leading `export ` — the `sed` above converts `test_env/dev.env`'s `export KEY=VALUE` lines (needed for `source` in "Running locally") into the format `--env-file` accepts.

Map the port to whatever `WEBHOOK_PORT` you set in `test_env/dev.env` — the container's `HEALTHCHECK` polls `http://localhost:${WEBHOOK_PORT}/health`.

## Tests

```bash
cargo test --workspace
```

## Dependency hygiene

- `Cargo.lock` currently pins duplicate major versions of a few transitive crates (e.g. two `http` majors pulled in by `axum`/`reqwest`/`teloxide`, three `rand` majors, multiple `windows-sys` majors, two `syn` majors via `thiserror-impl`). This is expected in a 380+ package dependency tree with several independently-versioned ecosystems (axum vs. teloxide vs. reqwest) and isn't independently fixable without upstream changes.
- Periodically run `cargo update` and `cargo tree -d` to check whether any duplication has become prunable, and `cargo tree -i <crate>@<version>` to find which direct dependency pulls in a specific duplicate.
```

- [ ] **Step 2: Sanity-check the env var table against `config.rs`**

Run:
```bash
grep -n 'get_env(' book_bot/src/config.rs
```
Expected: every `get_env("...")` argument in the output appears as a "yes" row in the README table (16 calls covering the 15 required variables above; `TELEGRAM_BOT_API_ROOT` is wrapped in `reqwest::Url::parse` but still sourced via `get_env`).

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add root README with architecture, env vars, and run instructions"
```

## Self-review notes

- Spec coverage: 13.1 → Task 5; 13.2 → Tasks 1-2; 13.3 → Task 3; 13.4 → Task 4; 13.5 → documented in Task 5's README content (no fix exists per spec); 13.6 → already satisfied, called out explicitly so the executor doesn't duplicate work.
- Acceptance criteria: "README exists and suffices to start from scratch" → Task 5; "`dateparser` and `strum_macros` gone from Cargo.toml, build/tests green" → Tasks 3-4, each with an explicit `cargo build`/`cargo test` step; "error types derive `thiserror::Error` with human-readable messages" → Tasks 1-2.
