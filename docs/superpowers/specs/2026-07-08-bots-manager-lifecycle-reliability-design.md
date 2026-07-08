# Spec 04: bots_manager reliability — webhook server, bot list sync, graceful shutdown

- **Priority:** high
- **Effort:** L (a group of related M/S tasks)
- **Category:** reliability
- **Source:** `docs/specs/04-bots-manager-lifecycle-reliability.md`

## Problem

### 4.1. Webhook server crash leaves the process a "zombie"

`book_bot/src/bots_manager/axum_server.rs:128-150` (in `start_axum_server`) — the server is started in `tokio::spawn`, the `JoinHandle` is dropped, and inside there are `bind(addr).await.unwrap()` and `serve(...).await.unwrap()`. If the port is taken or `serve` fails, the panic aborts the *whole process* (the workspace `Cargo.toml` sets `panic = "abort"`, see Risks/notes) rather than failing in a controlled way, and there is no way for `BotsManager::start`'s caller to know the server never came up if the panic happens to be swallowed by an intermediate layer.

### 4.2. `BOTS_DATA` is never refreshed or cleaned up

`book_bot/src/bots_manager/mod.rs:96-106` — `check_bots_data` skips already-known tokens (`contains_key → continue`), and the cache has no TTL:
- a change of a bot's `BotCache` setting in the manager is not picked up until restart;
- a bot deleted from the manager keeps serving webhooks indefinitely (the only removal path is the "Invalid bot token" branch in `check_pending_updates`, `mod.rs:201-210`).

### 4.3. The webhook-check circuit breaker is dead code

`book_bot/src/bots_manager/mod.rs:59-64,168-224` — `WEBHOOK_CHECK_ERRORS_COUNT` lives 600s (TTI), while `check_pending_updates` runs every 1800s — the entry is guaranteed to expire between runs, so the `>= 3` threshold is unreachable.

### 4.4. Graceful shutdown is a blind `sleep(5)`

`book_bot/src/bots_manager/internal.rs:164-171`, `mod.rs:158-166`, `main.rs:44-52`:
- the per-bot dispatcher `JoinHandle` (from `tokio::spawn` in `internal.rs:164`) is dropped — there is no way to await actual completion of update processing;
- `stop_all` does `stop_token.stop()` + `invalidate_all()` + `sleep(5s)` "and hope": long handlers (book downloads) get killed, and moka's eviction listener runs as part of a lazily-driven maintenance task, so `sender.close()` may not even run within those 5 seconds;
- the shutdown signal is detected via the `ctrlc` crate setting an `AtomicBool`, polled every 1s in both `BotsManager::start`'s loop and `start_axum_server`'s `with_graceful_shutdown` future.

### 4.5. `max_capacity(100)` on `BOTS_ROUTES` evicts live bots

`book_bot/src/bots_manager/mod.rs:72-88` — with a fleet close to 100 bots, size-based eviction starts stopping dispatchers of active bots; updates queued in the unbounded channel are lost. The eviction listener's `cause` parameter is discarded (`_cause`), so idle vs. size eviction can't be told apart from logs.

### 4.6. The manager-API client does not check response statuses

`book_bot/src/bots_manager/bot_manager_client.rs:30-54` — `get_bots` on 401/500 yields a cryptic JSON deserialization error; `delete_bot` returns `Ok(())` for any status (the bot is considered deleted while it is not).

### 4.7. Minor related items

- `mod.rs:140-149` — manager unavailability is logged as `log::info!("{err:?}")` with no context and no metric.
- `mod.rs:183-199` — the result of the repeated `set_webhook` is ignored; when `pending_update_count > 0` with an empty `last_error_message` (webhook removed manually) nothing happens.
- `internal.rs:141-155` — `set_my_commands` runs on every bot "wake-up" after TTI eviction (an extra network call delaying the first update; flood-limit risk on mass wake-ups).
- `internal.rs:37` — unbounded update channel: with a slow handler the queue grows without limit.
- `mod.rs:133-138` — `loop { if join_next().await.is_none() { break } }` instead of `while let Some(res) = join_next().await`, and any `JoinError` from a set-webhook task is silently dropped.

**Explicitly out of scope for this pass:** `internal.rs:63-70`'s webhook URL is built by string concatenation (`{base_url}:{port}/{token}/`), and the same port doubles as both the local bind port and the public-facing port. Fixing this needs a new env var (splitting "public webhook URL" from "local bind port") and is deferred to a separate deployment-coordinated change.

## Proposed solution

### Shutdown coordination (new shared mechanism)

Replace `Arc<AtomicBool>` + 1s polling with a `tokio::sync::watch::channel(())` shutdown signal. One task in `main.rs` runs `tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = sigterm.recv() => {} }` (`tokio::signal::unix::signal(SignalKind::terminate())`) and sends on the watch sender once. This replaces the `ctrlc` crate entirely (dependency removed from `Cargo.toml`). Add `"signal"` to `book_bot`'s explicit `tokio` feature list in `Cargo.toml` (it's already pulled in transitively via `signal-hook-registry` in `Cargo.lock`, but declaring it explicitly avoids relying on an implicit transitive feature).

Both `start_axum_server`'s graceful-shutdown future and `BotsManager::start`'s tick loop hold a clone of the watch `Receiver` and react via `receiver.changed()` inside a `tokio::select!` against their existing per-tick work — shutdown is detected immediately instead of on the next 1s tick.

`BOTS_ROUTES`'s value tuple gains a fourth field: `tokio::task::JoinHandle<()>` for the dispatcher task spawned in `internal.rs:164`.

`stop_all()` becomes:
1. Iterate `BOTS_ROUTES`, call `stop_token.stop()` on each, and collect the `JoinHandle`s into a `Vec`.
2. `BOTS_ROUTES.invalidate_all()` (fires the existing eviction listener — closes senders; redundant-but-harmless second `stop()` calls are fine, `StopToken::stop()` is idempotent).
3. `tokio::time::timeout(SHUTDOWN_TIMEOUT, futures::future::join_all(handles)).await` — `SHUTDOWN_TIMEOUT` is a named constant, default 10s (up from the current blind 5s, since it now represents "let in-flight handlers actually finish" rather than "hope"). On timeout, log a warning with how many dispatchers didn't finish in the deadline; either way, return afterward — the process exits regardless of whether every handler drained (K8s SIGKILLs the process past its own termination grace period anyway, so this is a best-effort window, not a guarantee).

### 4.1 — webhook server startup

Move `TcpListener::bind` out of the `tokio::spawn` and into `start_axum_server` directly, before the router is spawned. A bind failure returns a clear log line and the process exits (`std::process::exit(1)`) before `BotsManager::start` ever enters its loop — no bots get initialized against a webhook server that can't run. The `axum::serve(...).with_graceful_shutdown(...)` call keeps running inside `tokio::spawn`, but the `JoinHandle` is now returned up to `BotsManager::start`, which checks `handle.is_finished()` each tick. The server task is only expected to finish after the shutdown watch signal has fired; if it finishes while the process is still meant to be running, that's a crash — log an error and exit the process.

### 4.2 — BOTS_DATA full sync

`check_bots_data` becomes a real diff each cycle:
- Upsert every bot from the fresh list into `BOTS_DATA`, unconditionally (not gated on `contains_key`), so a changed `BotCache` value is picked up.
- Any key currently in `BOTS_DATA` that is absent from the fresh list gets `BOTS_DATA.invalidate(token)` and `BOTS_ROUTES.remove(token)` (the existing eviction listener stops the token and closes the sender — no new listener logic needed).

### 4.3 — circuit breaker

`WEBHOOK_CHECK_ERRORS_COUNT` drops `time_to_idle` entirely (matches the no-TTL style already used by `BOTS_DATA`/`INITED_BOTS_IDS`). In `check_pending_updates`, the `Ok(webhook_info)` branch resets the counter to 0 (a successful `get_webhook_info` call clears the breaker, regardless of `pending_update_count`); the existing `Err(err)` branch keeps incrementing as today.

### 4.5 — BOTS_ROUTES capacity

Drop `.max_capacity(100)` from the `BOTS_ROUTES` cache builder — only idle (`time_to_idle`) eviction remains. The eviction listener's `cause` parameter is logged instead of discarded.

### 4.6 — manager client status checks

`get_bots` and `delete_bot` both call `.error_for_status()?` on the `reqwest::Response` before parsing the body / returning success.

### 4.7 — minor items

- `mod.rs:140-149`: `log::error!` (was `info!`) with the error in context, plus `metrics::counter!("bots_manager_fetch_failures_total").increment(1)` (follows the `_total` counter convention already used by `webhook_secret_rejected_total`).
- `mod.rs:183-199`: check `set_webhook`'s bool result and log a failure; also treat `webhook_info.url.is_none()` as needing a re-set (today only `last_error_message` triggers it).
- `internal.rs:141-155`: gate `set_my_commands` behind `INITED_BOTS_IDS` so it runs once per bot id, not on every TTI wake-up (the cache already exists for this exact "have we inited this bot" purpose).
- `internal.rs:37`: `mpsc::unbounded_channel()` → bounded `mpsc::channel(1024)`. `ClosableSender<T>` (`closable_sender.rs`) is generalized from hardcoded `mpsc::UnboundedSender<T>` to `mpsc::Sender<T>`. The axum handler (`axum_server.rs`) sends via `try_send` (non-blocking) rather than `.send().await`, so a full queue can't stall the webhook HTTP response — a full queue logs and returns `StatusCode::SERVICE_UNAVAILABLE` (Telegram retries webhook delivery on non-2xx).
- `mod.rs:133-138`: `while let Some(res) = set_webhook_tasks.join_next().await { if let Err(join_err) = res { log::error!(...) } }`.

## Testing approach

Per-module `#[cfg(test)]` blocks, matching the existing convention (e.g. `book_bot/src/bots/approved_bot/services/rate_limit.rs`, `book_bot/src/handler_metrics.rs`) — `book_bot` is binary-only with no `tests/` integration directory.

- `check_bots_data`'s diff logic (add / update `BotCache` / remove) is tested directly against fixture `BotData` lists and the real `BOTS_DATA`/`BOTS_ROUTES` statics (each test uses distinct fake tokens to avoid cross-test interference, following existing test patterns in this codebase where global caches are shared statics).
- The circuit breaker's reset-on-success and threshold behavior is tested directly against `WEBHOOK_CHECK_ERRORS_COUNT`.
- `stop_all`'s timeout math (returns once all handles finish; gives up after `SHUTDOWN_TIMEOUT`) is tested with a small harness of dummy `tokio::spawn` tasks (one that finishes immediately, one that sleeps past the timeout) standing in for real dispatchers — this avoids needing a real Telegram bot/dispatcher in the test.
- `bot_manager_client`'s `error_for_status` behavior is tested against a local mock HTTP server (check existing dev-dependencies for a mock server crate already in use elsewhere in the workspace before adding one).

Signal wiring itself (`ctrl_c`/SIGTERM → watch channel) is integration-shaped and not covered by a unit test; it's verified manually (see Acceptance criteria).

## Acceptance criteria

- A taken port → the process fails at startup with a clear error (instead of living without a server).
- Deleting a bot in the manager → the bot stops being served within one `check` cycle; a `BotCache` change is picked up within one cycle.
- SIGTERM: the process waits for active handlers (with a timeout) and exits without losing already-accepted updates (verified manually: send SIGTERM to a running instance mid-download and confirm the download completes before exit, or the timeout warning is logged).
- Test/simulation: three consecutive `check_pending_updates` failures for one bot → the bot is skipped in subsequent checks; a subsequent success resets the counter (the circuit breaker works both ways).

## Risks / notes

- The workspace `Cargo.toml` sets `panic = "abort"` (see `docs/superpowers/plans/2026-07-07-panic-safety.md`): a panic anywhere aborts the *entire* process immediately, with no per-task isolation. This design leans on that constraint rather than fighting it — failures are turned into controlled `log::error!` + `std::process::exit(1)` paths *before* they'd otherwise panic, rather than assuming a panicked task can be caught and the rest of the process kept alive.
- Removing the `ctrlc` dependency and switching to `tokio::signal` changes process-shutdown behavior slightly: `ctrlc`'s `termination` feature already covered SIGTERM+SIGINT on Unix, so this is a like-for-like replacement, not new signal coverage.
- Bounding the per-bot update channel (4.7) is a behavior change under sustained slow-handler load: instead of growing unboundedly, a full queue now makes the webhook respond 503 to Telegram (which retries later) via `try_send`, trading unbounded memory growth for occasional Telegram-side retries. Worth calling out in the PR description.
- The webhook URL/port split (env var change) is explicitly deferred — see "Explicitly out of scope" above.
