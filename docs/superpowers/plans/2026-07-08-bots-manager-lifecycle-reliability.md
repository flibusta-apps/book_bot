# bots_manager Lifecycle Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `bots_manager` fail fast on startup problems, keep its bot list and webhook state correctly synchronized with the manager API, and shut down without dropping in-flight work — replacing today's zombie-on-crash server, dead circuit breaker, and blind 5-second shutdown sleep.

**Architecture:** `BOTS_ROUTES` (a `moka::future::Cache` keyed by bot token) gains the per-bot dispatcher's `JoinHandle`, wrapped in `Arc` (moka requires `V: Clone`, and `tokio::task::JoinHandle` itself is not `Clone`). Process shutdown moves from `Arc<AtomicBool>` polled every second to a `tokio::sync::watch::channel(())` fed by one `tokio::select!` on `tokio::signal::ctrl_c()`/SIGTERM, replacing the `ctrlc` crate. `stop_all()` signals every bot's `StopToken`, then polls the collected `JoinHandle`s' `is_finished()` every 100ms up to a 10s deadline (no `.await` needed on the handles themselves, sidestepping the `Arc`/ownership issue). The webhook axum server binds its `TcpListener` before spawning (surfacing bind errors as a controlled `std::process::exit(1)` instead of a panic buried in a detached task), and returns its own `JoinHandle` so the main loop can detect an unexpected server-task exit and terminate the process instead of running headless. `BOTS_DATA` synchronization becomes a real diff each cycle (upsert + invalidate-if-absent) instead of insert-only. The webhook-check circuit breaker counter loses its TTL and resets on success instead of expiring before it can ever trip.

**Tech Stack:** Rust, `tokio` (`sync::watch`, `signal::unix`, `task::JoinHandle`), `moka::future::Cache`, `teloxide` 0.17 (`StopToken`/`StopFlag`/`Dispatcher`), `metrics` 0.24 (custom `Recorder` test double, matching `book_bot/src/handler_metrics.rs`'s existing pattern), `reqwest` (`Response::from(http::Response<...>)` for network-free response tests).

## Global Constraints

- `book_bot` is a binary-only crate (no `[lib]` target, no `tests/` directory) — every test lives in a `#[cfg(test)] mod tests { ... }` block inside the `src/*.rs` file it tests, matching existing files (`book_bot/src/handler_metrics.rs`, `book_bot/src/bots_manager/utils.rs`, `book_bot/src/bots/approved_bot/services/rate_limit.rs`).
- New dependencies are kept to a minimum: add `"signal"` to the already-declared `tokio` feature list in `book_bot/Cargo.toml` (the feature is already pulled in transitively via `signal-hook-registry` per `Cargo.lock`, so this makes an existing capability explicit rather than adding new code); remove the `ctrlc` dependency entirely (superseded by `tokio::signal`); add `http = "1"` under a new `[dev-dependencies]` section (already present in `Cargo.lock` at that version via `reqwest`/`axum`, used only in tests to construct `reqwest::Response` values without a network call).
- `BOTS_DATA`, `BOTS_ROUTES`, `WEBHOOK_CHECK_ERRORS_COUNT`, `INITED_BOTS_IDS`, `COMMANDS_SET_BOT_IDS` are process-wide `LazyLock` singletons shared by every test in the binary. Tests that touch them **must** use unique fake keys (distinct fake tokens / large distinguishing bot ids like `900_0xx`) so parallel `cargo test` runs never collide, and must call `.run_pending_tasks().await` on the cache after an `invalidate`/`remove` when the assertion depends on the eviction listener having actually run (moka defers eviction-listener notification to a maintenance step; `run_pending_tasks()` forces it synchronously).
- The workspace's `[profile.release]` sets `panic = "abort"` (see `docs/superpowers/plans/2026-07-07-panic-safety.md`) — this does **not** apply to `cargo test` (which uses the default `dev`/`test` profile, `panic = "unwind"`), so tests may safely spawn tasks that panic without aborting the test binary. Production code added by this plan must not rely on catching a panic in a spawned task — startup failures are surfaced via `Result`/`log::error!` + `std::process::exit(1)` before anything would panic.
- Follow existing logging/metrics conventions: `tracing::log::{info,warn,error}!` (already imported as `use tracing::log;` in every touched file), `metrics::counter!("name_total"[, "label" => value]).increment(n)` (snake_case, `_total` suffix, see `book_bot/src/bots_manager/axum_server.rs`'s `webhook_secret_rejected_total`).
- Out of scope for this plan: the webhook URL/local-port split described in the source spec's `internal.rs:63-70` item (needs a new deployment-coordinated env var; deferred per the design doc).

---

## Task 1: Bound the per-bot update channel

**Files:**
- Modify: `book_bot/src/bots_manager/closable_sender.rs`
- Modify: `book_bot/src/bots_manager/internal.rs:14-51` (`get_listener`)
- Modify: `book_bot/src/bots_manager/axum_server.rs:107-126` (`telegram_request`'s send call)
- Test: inline in `book_bot/src/bots_manager/closable_sender.rs`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `ClosableSender<T>` generalized to wrap `tokio::sync::mpsc::Sender<T>` (was `UnboundedSender<T>`) — `internal::get_listener`'s return type and `axum_server.rs`'s handler both depend on this from here on.

Today `internal.rs:37` uses an unbounded channel between the axum webhook handler and each bot's teloxide dispatcher — under a slow handler (e.g. a stuck book download) the queue grows without limit. This task bounds it and makes the webhook handler respond with backpressure (503, so Telegram retries) instead of buffering forever.

- [ ] **Step 1: Generalize `ClosableSender` to a bounded sender and write its test**

Replace the full contents of `book_bot/src/bots_manager/closable_sender.rs`:

```rust
use tokio::sync::mpsc;

pub struct ClosableSender<T> {
    origin: std::sync::Arc<std::sync::RwLock<Option<mpsc::Sender<T>>>>,
}

impl<T> Clone for ClosableSender<T> {
    fn clone(&self) -> Self {
        Self {
            origin: self.origin.clone(),
        }
    }
}

impl<T> ClosableSender<T> {
    pub fn new(sender: mpsc::Sender<T>) -> Self {
        Self {
            origin: std::sync::Arc::new(std::sync::RwLock::new(Some(sender))),
        }
    }

    pub fn get(&self) -> Option<mpsc::Sender<T>> {
        self.origin
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn close(&mut self) {
        self.origin
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_returns_sender_until_closed() {
        let (tx, mut rx) = mpsc::channel::<i32>(1);
        let mut closable = ClosableSender::new(tx);

        let sender = closable.get().expect("sender should be available");
        sender.try_send(42).unwrap();
        assert_eq!(rx.recv().await, Some(42));

        closable.close();
        assert!(closable.get().is_none());
    }

    #[test]
    fn try_send_fails_when_full() {
        let (tx, _rx) = mpsc::channel::<i32>(1);
        tx.try_send(1).unwrap();

        assert!(matches!(
            tx.try_send(2),
            Err(mpsc::error::TrySendError::Full(2))
        ));
    }
}
```

- [ ] **Step 2: Run the new tests to confirm they fail to compile against the old unbounded-only usage sites**

Run: `cargo test -p book_bot --lib closable_sender 2>&1 | tail -40`
Expected: compile error, something like "expected `Sender<T>`, found `UnboundedSender<...>`" from `internal.rs`'s `get_listener`, since it still constructs an unbounded channel and passes it to `ClosableSender::new`.

- [ ] **Step 3: Switch `get_listener` to a bounded channel**

In `book_bot/src/bots_manager/internal.rs`, replace:

```rust
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
```

with:

```rust
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
```

and replace:

```rust
type UpdateSender = mpsc::UnboundedSender<Result<Update, std::convert::Infallible>>;

pub fn get_listener() -> (
    StopToken,
    StopFlag,
    UnboundedSender<Result<Update, std::convert::Infallible>>,
    impl UpdateListener<Err = Infallible>,
) {
    let (tx, rx): (UpdateSender, _) = mpsc::unbounded_channel();

    let (stop_token, stop_flag) = mk_stop_token();

    let stream = UnboundedReceiverStream::new(rx);

    let listener = StatefulListener::new(
        (stream, stop_token.clone()),
        tuple_first_mut,
        |state: &mut (_, StopToken)| state.1.clone(),
    );

    (stop_token, stop_flag, tx, listener)
}
```

with:

```rust
pub const UPDATE_CHANNEL_CAPACITY: usize = 1024;

type UpdateSender = mpsc::Sender<Result<Update, std::convert::Infallible>>;

pub fn get_listener() -> (
    StopToken,
    StopFlag,
    UpdateSender,
    impl UpdateListener<Err = Infallible>,
) {
    let (tx, rx): (UpdateSender, _) = mpsc::channel(UPDATE_CHANNEL_CAPACITY);

    let (stop_token, stop_flag) = mk_stop_token();

    let stream = ReceiverStream::new(rx);

    let listener = StatefulListener::new(
        (stream, stop_token.clone()),
        tuple_first_mut,
        |state: &mut (_, StopToken)| state.1.clone(),
    );

    (stop_token, stop_flag, tx, listener)
}
```

- [ ] **Step 4: Switch the webhook handler from `send` to `try_send`**

In `book_bot/src/bots_manager/axum_server.rs`, replace:

```rust
        match serde_json::from_str::<Update>(&input) {
            Ok(mut update) => {
                if let UpdateKind::Error(value) = &mut update.kind {
                    *value = serde_json::from_str(&input).unwrap_or_default();
                }

                if let Err(err) = tx.send(Ok(update)) {
                    log::error!("{err:?}");
                    BOTS_ROUTES.remove(&token).await;
                    return StatusCode::SERVICE_UNAVAILABLE;
                }
            }
            Err(error) => {
```

with:

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
                        log::error!("Update channel closed for Bot(token={})", mask_token(&token));
                        BOTS_ROUTES.remove(&token).await;
                        return StatusCode::SERVICE_UNAVAILABLE;
                    }
                }
            }
            Err(error) => {
```

This preserves the original behavior on a closed channel (remove the stale route) while adding a new, correct behavior for a merely-full channel: drop the update and let Telegram retry, without tearing down an otherwise-healthy bot.

- [ ] **Step 5: Run the full test suite**

Run: `cargo test -p book_bot`
Expected: PASS (including the two new `closable_sender` tests), and `cargo build -p book_bot` succeeds with no leftover references to `UnboundedSender`/`UnboundedReceiverStream`.

- [ ] **Step 6: Commit**

```bash
git add book_bot/src/bots_manager/closable_sender.rs book_bot/src/bots_manager/internal.rs book_bot/src/bots_manager/axum_server.rs
git commit -m "$(cat <<'EOF'
fix(bots_manager): bound the per-bot update channel and backpressure via 503

An unbounded mpsc channel between the webhook handler and each bot's
dispatcher let the queue grow without limit under a slow handler. Switch
to a bounded channel; a full queue now makes the webhook respond 503 so
Telegram retries, instead of buffering forever.
EOF
)"
```

---

## Task 2: Manager API client checks response status

**Files:**
- Modify: `book_bot/src/bots_manager/bot_manager_client.rs`
- Modify: `book_bot/Cargo.toml` (new `[dev-dependencies]` section)
- Test: inline in `book_bot/src/bots_manager/bot_manager_client.rs`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: nothing consumed by later tasks (this task is self-contained).

`get_bots` today feeds a 401/500 response straight into `.json()`, producing a confusing deserialization error instead of a clear HTTP-status error; `delete_bot` returns `Ok(())` for any status code, so a failed delete is silently treated as a success.

- [ ] **Step 1: Add the `http` dev-dependency for network-free response tests**

In `book_bot/Cargo.toml`, add a new section at the end of the file:

```toml

[dev-dependencies]
http = "1"
```

- [ ] **Step 2: Write the failing tests**

Append to `book_bot/src/bots_manager/bot_manager_client.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn response_with_status(status: u16) -> reqwest::Response {
        let http_response = http::Response::builder()
            .status(status)
            .body(Vec::<u8>::new())
            .unwrap();
        reqwest::Response::from(http_response)
    }

    #[tokio::test]
    async fn parse_bots_response_errors_on_401() {
        let response = response_with_status(401);
        assert!(parse_bots_response(response).await.is_err());
    }

    #[tokio::test]
    async fn parse_bots_response_parses_valid_json() {
        let http_response = http::Response::builder()
            .status(200)
            .body(br#"[{"id":1,"token":"abc","cache":"cache"}]"#.to_vec())
            .unwrap();
        let response = reqwest::Response::from(http_response);

        let bots = parse_bots_response(response).await.unwrap();
        assert_eq!(bots.len(), 1);
        assert_eq!(bots[0].id, 1);
        assert_eq!(bots[0].cache, BotCache::Cache);
    }

    #[test]
    fn check_delete_response_errors_on_500() {
        assert!(check_delete_response(response_with_status(500)).is_err());
    }

    #[test]
    fn check_delete_response_ok_on_200() {
        assert!(check_delete_response(response_with_status(200)).is_ok());
    }
}
```

Note: `BotCache` needs `PartialEq` for the `assert_eq!` above — it already derives it (`#[derive(Deserialize, Debug, PartialEq, Clone, Copy)]`).

- [ ] **Step 3: Run the tests to confirm they fail**

Run: `cargo test -p book_bot --lib bot_manager_client 2>&1 | tail -30`
Expected: compile error — `parse_bots_response` and `check_delete_response` don't exist yet.

- [ ] **Step 4: Extract the status-checking helpers and use them**

Replace `get_bots` and `delete_bot` in `book_bot/src/bots_manager/bot_manager_client.rs`:

```rust
pub async fn get_bots() -> Result<Vec<BotData>, reqwest::Error> {
    let response = CLIENT
        .get(&config::CONFIG.manager_url)
        .header("Authorization", &config::CONFIG.manager_api_key)
        .send()
        .await?;

    parse_bots_response(response).await
}

async fn parse_bots_response(response: reqwest::Response) -> Result<Vec<BotData>, reqwest::Error> {
    response.error_for_status()?.json::<Vec<BotData>>().await
}

pub async fn delete_bot(id: u32) -> Result<(), reqwest::Error> {
    let response = CLIENT
        .delete(format!("{}/{}/", config::CONFIG.manager_url, id))
        .header("Authorization", &config::CONFIG.manager_api_key)
        .send()
        .await?;

    check_delete_response(response)
}

fn check_delete_response(response: reqwest::Response) -> Result<(), reqwest::Error> {
    response.error_for_status()?;
    Ok(())
}
```

- [ ] **Step 5: Run the tests to confirm they pass**

Run: `cargo test -p book_bot --lib bot_manager_client`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add book_bot/Cargo.toml book_bot/src/bots_manager/bot_manager_client.rs
git commit -m "$(cat <<'EOF'
fix(bots_manager): check manager API response status before parsing

get_bots on a 401/500 previously surfaced as a confusing JSON
deserialization error; delete_bot returned Ok(()) for any status,
treating a failed delete as a success.
EOF
)"
```

---

## Task 3: Track each bot's dispatcher JoinHandle; remove BOTS_ROUTES's size limit; log real eviction cause

**Files:**
- Modify: `book_bot/src/bots_manager/mod.rs` (`StopTokenWithSender`, `BOTS_ROUTES`, `stop_all`)
- Modify: `book_bot/src/bots_manager/internal.rs` (`start_bot`)
- Modify: `book_bot/src/bots_manager/axum_server.rs` (`telegram_request`'s route lookup)
- Test: inline in `book_bot/src/bots_manager/mod.rs`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `StopTokenWithSender = (StopToken, StopFlag, ClosableSender<...>, Arc<tokio::task::JoinHandle<()>>)` — Task 8 (`stop_all` timeout join) depends on this 4-tuple shape and the `Arc` wrapping.

`moka::future::Cache<K, V>` requires `V: Clone`; `tokio::task::JoinHandle<T>` is not `Clone`, so it must be wrapped in `Arc` to live in `BOTS_ROUTES`'s value tuple. With a fleet near 100 bots, `BOTS_ROUTES`'s `max_capacity(100)` starts evicting live, active bots (losing their queued updates) — remove it; only idle (`time_to_idle`) eviction should apply. The eviction listener's `cause` argument is currently discarded; log it so idle vs. size eviction (and, later, explicit removal from the sync fix in Task 4) can be told apart.

- [ ] **Step 1: Write the failing test**

Append to `book_bot/src/bots_manager/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use teloxide::stop::mk_stop_token;

    #[tokio::test]
    async fn invalidating_a_route_stops_its_token_and_closes_its_sender() {
        let (stop_token, stop_flag) = mk_stop_token();
        let (tx, _rx) = tokio::sync::mpsc::channel::<Result<Update, std::convert::Infallible>>(1);
        let mut closable = ClosableSender::new(tx);
        let handle = Arc::new(tokio::spawn(async {}));

        let test_token = "test-route-invalidation-token".to_string();
        BOTS_ROUTES
            .insert(
                test_token.clone(),
                (stop_token, stop_flag.clone(), closable.clone(), handle),
            )
            .await;

        BOTS_ROUTES.invalidate(&test_token).await;
        BOTS_ROUTES.run_pending_tasks().await;

        assert!(stop_flag.is_stopped());
        assert!(closable.get().is_none());
    }
}
```

- [ ] **Step 2: Run the test to confirm it fails**

Run: `cargo test -p book_bot --lib bots_manager::mod::tests 2>&1 | tail -30`
Expected: compile error — `BOTS_ROUTES.insert` expects a 3-tuple, this passes a 4-tuple.

- [ ] **Step 3: Extend the tuple, drop the size limit, log the real cause**

In `book_bot/src/bots_manager/mod.rs`, replace:

```rust
type StopTokenWithSender = (
    StopToken,
    StopFlag,
    ClosableSender<Result<Update, std::convert::Infallible>>,
);

pub static BOTS_ROUTES: LazyLock<Cache<String, StopTokenWithSender>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(60 * 60))
        .max_capacity(100)
        .eviction_listener(|token: Arc<String>, value: StopTokenWithSender, _cause| {
            log::info!(
                "Stop Bot(token={})!",
                crate::bots_manager::utils::mask_token(&token)
            );

            let (stop_token, _stop_flag, mut sender) = value;

            stop_token.stop();
            sender.close();
        })
        .build()
});
```

with:

```rust
type StopTokenWithSender = (
    StopToken,
    StopFlag,
    ClosableSender<Result<Update, std::convert::Infallible>>,
    Arc<tokio::task::JoinHandle<()>>,
);

pub static BOTS_ROUTES: LazyLock<Cache<String, StopTokenWithSender>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(60 * 60))
        .eviction_listener(|token: Arc<String>, value: StopTokenWithSender, cause| {
            log::info!(
                "Stop Bot(token={}), cause={cause:?}!",
                crate::bots_manager::utils::mask_token(&token)
            );

            let (stop_token, _stop_flag, mut sender, _dispatcher_handle) = value;

            stop_token.stop();
            sender.close();
        })
        .build()
});
```

- [ ] **Step 4: Update `stop_all`'s destructuring for the 4-tuple**

In `book_bot/src/bots_manager/mod.rs`, replace:

```rust
    pub async fn stop_all() {
        for (_, (stop_token, _, _)) in BOTS_ROUTES.iter() {
            stop_token.stop();
        }

        BOTS_ROUTES.invalidate_all();

        sleep(Duration::from_secs(5)).await;
    }
```

with:

```rust
    pub async fn stop_all() {
        for (_, (stop_token, _, _, _)) in BOTS_ROUTES.iter() {
            stop_token.stop();
        }

        BOTS_ROUTES.invalidate_all();

        sleep(Duration::from_secs(5)).await;
    }
```

(Task 8 replaces this body entirely to actually use the `JoinHandle`s; this step just keeps it compiling.)

- [ ] **Step 5: Store the dispatcher's `JoinHandle` in `start_bot`**

In `book_bot/src/bots_manager/internal.rs`, add `use std::sync::Arc;` to the imports (near the top, alongside the other `std::` imports), then replace:

```rust
    let (stop_token, stop_flag, tx, listener) = get_listener();

    tokio::spawn(async move {
        dispatcher
            .dispatch_with_listener(
                listener,
                CustomErrorHandler::with_custom_text("An error from the update listener"),
            )
            .await;
    });

    BOTS_ROUTES
        .insert(
            token.to_string(),
            (stop_token, stop_flag, ClosableSender::new(tx)),
        )
        .await;
```

with:

```rust
    let (stop_token, stop_flag, tx, listener) = get_listener();

    let dispatcher_handle = Arc::new(tokio::spawn(async move {
        dispatcher
            .dispatch_with_listener(
                listener,
                CustomErrorHandler::with_custom_text("An error from the update listener"),
            )
            .await;
    }));

    BOTS_ROUTES
        .insert(
            token.to_string(),
            (stop_token, stop_flag, ClosableSender::new(tx), dispatcher_handle),
        )
        .await;
```

- [ ] **Step 6: Update the webhook handler's route destructuring for the 4-tuple**

In `book_bot/src/bots_manager/axum_server.rs`, replace:

```rust
        let (_, stop_flag, r_tx) = match BOTS_ROUTES.get(&token).await {
```

with:

```rust
        let (_, stop_flag, r_tx, _dispatcher_handle) = match BOTS_ROUTES.get(&token).await {
```

- [ ] **Step 7: Run the tests to confirm they pass**

Run: `cargo test -p book_bot`
Expected: PASS, including the new `invalidating_a_route_stops_its_token_and_closes_its_sender` test.

- [ ] **Step 8: Commit**

```bash
git add book_bot/src/bots_manager/mod.rs book_bot/src/bots_manager/internal.rs book_bot/src/bots_manager/axum_server.rs
git commit -m "$(cat <<'EOF'
fix(bots_manager): track dispatcher JoinHandles, drop BOTS_ROUTES size cap

max_capacity(100) started evicting live bots' dispatchers once the fleet
approached 100. Remove the size limit (idle eviction still applies), log
the real eviction cause, and store each dispatcher's JoinHandle so a
future graceful-shutdown fix can wait on it instead of guessing.
EOF
)"
```

---

## Task 4: BOTS_DATA full synchronization every cycle

**Files:**
- Modify: `book_bot/src/bots_manager/mod.rs` (`check_bots_data`)
- Test: inline in `book_bot/src/bots_manager/mod.rs`

**Interfaces:**
- Consumes: `BOTS_ROUTES` (Task 3's 4-tuple shape, only via `.remove(token)` — arity-agnostic).
- Produces: nothing consumed by later tasks.

`check_bots_data` today skips any token already present in `BOTS_DATA`, so a bot's `BotCache` setting change in the manager is never picked up, and a bot deleted from the manager keeps its `BOTS_DATA` entry (and keeps being served) forever. Fix: always upsert, and remove anything no longer in the fresh list.

- [ ] **Step 1: Write the failing test**

Append the following test function inside the `mod tests { ... }` block added in Task 3, in `book_bot/src/bots_manager/mod.rs` (after `invalidating_a_route_stops_its_token_and_closes_its_sender`):

```rust
    fn fake_route() -> StopTokenWithSender {
        let (stop_token, stop_flag) = mk_stop_token();
        let (tx, _rx) = tokio::sync::mpsc::channel::<Result<Update, std::convert::Infallible>>(1);
        let handle = Arc::new(tokio::spawn(async {}));
        (stop_token, stop_flag, ClosableSender::new(tx), handle)
    }

    #[tokio::test]
    async fn sync_upserts_changes_and_removes_bots_absent_from_the_fresh_list() {
        let kept_token = "sync-test-kept-token".to_string();
        let removed_token = "sync-test-removed-token".to_string();

        BOTS_DATA
            .insert(
                kept_token.clone(),
                BotData {
                    id: 101,
                    token: kept_token.clone(),
                    cache: BotCache::Cache,
                },
            )
            .await;
        BOTS_DATA
            .insert(
                removed_token.clone(),
                BotData {
                    id: 102,
                    token: removed_token.clone(),
                    cache: BotCache::Cache,
                },
            )
            .await;
        BOTS_ROUTES.insert(removed_token.clone(), fake_route()).await;

        let fresh = vec![BotData {
            id: 101,
            token: kept_token.clone(),
            cache: BotCache::NoCache,
        }];
        BotsManager::check_bots_data(&fresh).await;
        BOTS_DATA.run_pending_tasks().await;
        BOTS_ROUTES.run_pending_tasks().await;

        assert!(!BOTS_DATA.contains_key(&removed_token));
        assert!(!BOTS_ROUTES.contains_key(&removed_token));

        let kept = BOTS_DATA
            .get(&kept_token)
            .await
            .expect("kept bot should remain");
        assert_eq!(kept.cache, BotCache::NoCache);
    }
```

- [ ] **Step 2: Run the test to confirm it fails**

Run: `cargo test -p book_bot --lib bots_manager::mod::tests::sync_upserts 2>&1 | tail -30`
Expected: FAIL — the removed bot is still present in `BOTS_DATA` (the current `contains_key → continue` skip logic never removes or updates anything).

- [ ] **Step 3: Rewrite `check_bots_data` as a real diff**

Replace in `book_bot/src/bots_manager/mod.rs`:

```rust
    async fn check_bots_data(bots: &[BotData]) {
        for bot_data in bots.iter() {
            if BOTS_DATA.contains_key(&bot_data.token) {
                continue;
            }

            let bot_data: BotData = bot_data.clone();

            BOTS_DATA.insert(bot_data.token.clone(), bot_data).await;
        }
    }
```

with:

```rust
    async fn check_bots_data(bots: &[BotData]) {
        let fresh_tokens: std::collections::HashSet<&str> =
            bots.iter().map(|bot_data| bot_data.token.as_str()).collect();

        for bot_data in bots.iter() {
            BOTS_DATA
                .insert(bot_data.token.clone(), bot_data.clone())
                .await;
        }

        let stale_tokens: Vec<String> = BOTS_DATA
            .iter()
            .filter(|(token, _)| !fresh_tokens.contains(token.as_str()))
            .map(|(token, _)| token.as_str().to_string())
            .collect();

        for token in stale_tokens {
            BOTS_DATA.invalidate(&token).await;
            BOTS_ROUTES.remove(&token).await;
        }
    }
```

- [ ] **Step 4: Run the test to confirm it passes**

Run: `cargo test -p book_bot --lib bots_manager::mod::tests::sync_upserts`
Expected: PASS.

- [ ] **Step 5: Run the full test suite**

Run: `cargo test -p book_bot`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add book_bot/src/bots_manager/mod.rs
git commit -m "$(cat <<'EOF'
fix(bots_manager): synchronize BOTS_DATA with the manager every cycle

check_bots_data used to skip any already-known token, so a BotCache
change was never picked up and a bot deleted from the manager kept
being served forever. Now every cycle upserts the fresh list and
removes (data + route) anything no longer in it.
EOF
)"
```

---

## Task 5: Fix the webhook-check circuit breaker and the re-set-webhook logic

**Files:**
- Modify: `book_bot/src/bots_manager/mod.rs` (`WEBHOOK_CHECK_ERRORS_COUNT`, `check_pending_updates`)
- Test: inline in `book_bot/src/bots_manager/mod.rs`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: nothing consumed by later tasks.

`WEBHOOK_CHECK_ERRORS_COUNT` lives 600s (TTI) while `check_pending_updates` runs every 1800s, so the entry always expires between runs and the `>= 3` threshold is unreachable. Separately, when `pending_update_count > 0` but `last_error_message` is empty (webhook removed manually), nothing re-sets the webhook.

- [ ] **Step 1: Write the failing test**

Append inside the `mod tests { ... }` block in `book_bot/src/bots_manager/mod.rs` (after the test added in Task 4):

```rust
    #[tokio::test]
    async fn breaker_trips_after_three_failures_and_resets_on_success() {
        let bot_id = 900_001;

        assert!(!webhook_check_breaker_tripped(bot_id).await);

        record_webhook_check_failure(bot_id).await;
        record_webhook_check_failure(bot_id).await;
        assert!(!webhook_check_breaker_tripped(bot_id).await);

        record_webhook_check_failure(bot_id).await;
        assert!(webhook_check_breaker_tripped(bot_id).await);

        record_webhook_check_success(bot_id).await;
        assert!(!webhook_check_breaker_tripped(bot_id).await);
    }

    fn webhook_info(
        url: Option<&str>,
        pending: u32,
        last_error: Option<&str>,
    ) -> teloxide::types::WebhookInfo {
        teloxide::types::WebhookInfo {
            url: url.map(|u| reqwest::Url::parse(u).unwrap()),
            has_custom_certificate: false,
            pending_update_count: pending,
            ip_address: None,
            last_error_date: None,
            last_error_message: last_error.map(String::from),
            last_synchronization_error_date: None,
            max_connections: None,
            allowed_updates: None,
        }
    }

    #[test]
    fn no_action_when_no_pending_updates() {
        let info = webhook_info(Some("https://example.com/token/"), 0, Some("boom"));
        assert!(matches!(decide_webhook_action(&info), WebhookAction::NoAction));
    }

    #[test]
    fn reset_when_url_missing_despite_pending_updates() {
        let info = webhook_info(None, 5, None);
        assert!(matches!(
            decide_webhook_action(&info),
            WebhookAction::ReSet { url_missing: true }
        ));
    }

    #[test]
    fn reset_when_last_error_present() {
        let info = webhook_info(Some("https://example.com/token/"), 5, Some("boom"));
        assert!(matches!(
            decide_webhook_action(&info),
            WebhookAction::ReSet { url_missing: false }
        ));
    }

    #[test]
    fn no_action_when_pending_but_no_error_and_url_present() {
        let info = webhook_info(Some("https://example.com/token/"), 5, None);
        assert!(matches!(decide_webhook_action(&info), WebhookAction::NoAction));
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p book_bot --lib bots_manager::mod::tests 2>&1 | tail -30`
Expected: compile error — `webhook_check_breaker_tripped`, `record_webhook_check_failure`, `record_webhook_check_success`, `decide_webhook_action`, and `WebhookAction` don't exist yet.

- [ ] **Step 3: Drop the TTL and add the reset/threshold helpers**

Replace in `book_bot/src/bots_manager/mod.rs`:

```rust
pub static WEBHOOK_CHECK_ERRORS_COUNT: LazyLock<Cache<u32, u32>> = LazyLock::new(|| {
    Cache::builder()
        .time_to_idle(Duration::from_secs(600))
        .max_capacity(128)
        .build()
});
```

with:

```rust
pub static WEBHOOK_CHECK_ERRORS_COUNT: LazyLock<Cache<u32, u32>> =
    LazyLock::new(|| Cache::builder().build());
```

Then, just above `pub struct BotsManager;`, add:

```rust
async fn record_webhook_check_success(bot_id: u32) {
    WEBHOOK_CHECK_ERRORS_COUNT.insert(bot_id, 0).await;
}

async fn record_webhook_check_failure(bot_id: u32) -> u32 {
    let error_count = WEBHOOK_CHECK_ERRORS_COUNT.get(&bot_id).await.unwrap_or(0) + 1;
    WEBHOOK_CHECK_ERRORS_COUNT.insert(bot_id, error_count).await;
    error_count
}

async fn webhook_check_breaker_tripped(bot_id: u32) -> bool {
    WEBHOOK_CHECK_ERRORS_COUNT.get(&bot_id).await.unwrap_or(0) >= 3
}

enum WebhookAction {
    NoAction,
    ReSet { url_missing: bool },
}

fn decide_webhook_action(webhook_info: &teloxide::types::WebhookInfo) -> WebhookAction {
    if webhook_info.pending_update_count == 0 {
        return WebhookAction::NoAction;
    }

    if webhook_info.url.is_none() {
        return WebhookAction::ReSet { url_missing: true };
    }

    if let Some(ref err_msg) = webhook_info.last_error_message {
        if is_expected_telegram_error(err_msg) {
            log::warn!("Webhook last error (expected): {err_msg}");
        } else {
            log::error!("Webhook last error: {err_msg}");
        }
        return WebhookAction::ReSet { url_missing: false };
    }

    WebhookAction::NoAction
}
```

- [ ] **Step 4: Rewrite `check_pending_updates` to use the helpers**

Replace the whole function in `book_bot/src/bots_manager/mod.rs`:

```rust
    pub async fn check_pending_updates() {
        for (token, bot_data) in BOTS_DATA.iter() {
            let error_count = WEBHOOK_CHECK_ERRORS_COUNT
                .get(&bot_data.id)
                .await
                .unwrap_or(0);

            if error_count >= 3 {
                continue;
            }

            let bot = Bot::new(token.clone().as_str())
                .set_api_url(crate::config::CONFIG.telegram_bot_api.clone())
                .throttle(Limits::default());

            let result = bot.get_webhook_info().send().await;

            match result {
                Ok(webhook_info) => {
                    if webhook_info.pending_update_count == 0 {
                        continue;
                    }

                    if let Some(ref err_msg) = webhook_info.last_error_message {
                        if is_expected_telegram_error(err_msg) {
                            log::warn!("Webhook last error (expected): {err_msg}");
                        } else {
                            log::error!("Webhook last error: {err_msg}");
                        }

                        set_webhook(&bot_data).await;
                    }
                }
                Err(err) => {
                    let error_message = err.to_string();

                    if error_message.contains("Invalid bot token") {
                        BOTS_DATA.invalidate(token.as_str()).await;
                        if let Err(d_err) = delete_bot(bot_data.id).await {
                            log::error!("Error deleting bot {}: {:?}", bot_data.id, d_err);
                        };
                        continue;
                    }

                    if is_expected_telegram_error(&error_message) {
                        log::warn!("Error getting webhook info (expected): {error_message}");
                    } else {
                        log::error!("Error getting webhook info: {error_message}");
                    }

                    WEBHOOK_CHECK_ERRORS_COUNT
                        .insert(bot_data.id, error_count + 1)
                        .await;
                }
            }
        }
    }
```

with:

```rust
    pub async fn check_pending_updates() {
        for (token, bot_data) in BOTS_DATA.iter() {
            if webhook_check_breaker_tripped(bot_data.id).await {
                continue;
            }

            let bot = Bot::new(token.clone().as_str())
                .set_api_url(crate::config::CONFIG.telegram_bot_api.clone())
                .throttle(Limits::default());

            let result = bot.get_webhook_info().send().await;

            match result {
                Ok(webhook_info) => {
                    record_webhook_check_success(bot_data.id).await;

                    match decide_webhook_action(&webhook_info) {
                        WebhookAction::NoAction => continue,
                        WebhookAction::ReSet { url_missing } => {
                            if url_missing {
                                log::warn!(
                                    "Webhook URL missing for Bot(id={}) despite pending updates; re-setting",
                                    bot_data.id
                                );
                            }

                            if !set_webhook(&bot_data).await {
                                log::error!(
                                    "Failed to re-set webhook for Bot(id={})",
                                    bot_data.id
                                );
                            }
                        }
                    }
                }
                Err(err) => {
                    let error_message = err.to_string();

                    if error_message.contains("Invalid bot token") {
                        BOTS_DATA.invalidate(token.as_str()).await;
                        if let Err(d_err) = delete_bot(bot_data.id).await {
                            log::error!("Error deleting bot {}: {:?}", bot_data.id, d_err);
                        };
                        continue;
                    }

                    if is_expected_telegram_error(&error_message) {
                        log::warn!("Error getting webhook info (expected): {error_message}");
                    } else {
                        log::error!("Error getting webhook info: {error_message}");
                    }

                    record_webhook_check_failure(bot_data.id).await;
                }
            }
        }
    }
```

- [ ] **Step 5: Run the tests to confirm they pass**

Run: `cargo test -p book_bot --lib bots_manager::mod::tests`
Expected: PASS.

- [ ] **Step 6: Run the full test suite**

Run: `cargo test -p book_bot`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add book_bot/src/bots_manager/mod.rs
git commit -m "$(cat <<'EOF'
fix(bots_manager): fix dead webhook-check circuit breaker

WEBHOOK_CHECK_ERRORS_COUNT's 600s TTL always expired before the next
1800s check_pending_updates run, so the >=3 threshold was unreachable.
Drop the TTL and reset the counter on success. Also re-set the webhook
when its URL is missing, not only when last_error_message is set.
EOF
)"
```

---

## Task 6: Minor cleanup — manager-fetch metric, set_my_commands runs once, JoinError is logged

**Files:**
- Modify: `book_bot/src/bots_manager/mod.rs` (`check`, `check_uninited`)
- Modify: `book_bot/src/bots_manager/internal.rs` (`start_bot`)
- Test: inline in both files

**Interfaces:**
- Consumes: `BOTS_ROUTES` insert call shape from Task 3 (`start_bot`'s 4-tuple insert).
- Produces: `COMMANDS_SET_BOT_IDS: Cache<u32, ()>` (new static in `mod.rs`) — not consumed elsewhere in this plan, but mirrors `INITED_BOTS_IDS`'s existing shape for future use.

Three independent small fixes: (1) manager-unavailable is logged at `info!` with no metric; (2) `set_my_commands` runs on every bot wake-up after TTI eviction, not just once; (3) the `join_next` drain loop silently discards `JoinError`s.

- [ ] **Step 1: Write the failing tests**

Append inside the `mod tests { ... }` block in `book_bot/src/bots_manager/mod.rs` (after the tests from Task 5):

```rust
    #[test]
    fn manager_fetch_failure_increments_metric() {
        use metrics::{
            Counter, CounterFn, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString,
            Unit,
        };
        use std::sync::Mutex as StdMutex;

        #[derive(Default)]
        struct CountingRecorder {
            count: Arc<StdMutex<u64>>,
        }

        struct SharedCounter(Arc<StdMutex<u64>>);

        impl CounterFn for SharedCounter {
            fn increment(&self, value: u64) {
                *self.0.lock().unwrap() += value;
            }
            fn absolute(&self, value: u64) {
                *self.0.lock().unwrap() = value;
            }
        }

        impl Recorder for CountingRecorder {
            fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
            fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
            fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

            fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
                if key.name() == "bots_manager_fetch_failures_total" {
                    Counter::from_arc(Arc::new(SharedCounter(self.count.clone())))
                } else {
                    Counter::noop()
                }
            }

            fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
                Gauge::noop()
            }

            fn register_histogram(&self, _: &Key, _: &Metadata<'_>) -> Histogram {
                Histogram::noop()
            }
        }

        let recorder = CountingRecorder::default();
        let count = recorder.count.clone();

        metrics::with_local_recorder(&recorder, || {
            record_manager_fetch_failure();
            record_manager_fetch_failure();
        });

        assert_eq!(*count.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn drains_all_set_webhook_tasks_including_ones_that_panic() {
        let mut set: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
        set.spawn(async {});
        set.spawn(async { panic!("boom") });
        set.spawn(async {});

        let mut panicked = 0;
        while let Some(res) = set.join_next().await {
            if res.is_err() {
                panicked += 1;
            }
        }

        assert_eq!(panicked, 1);
    }
```

Append a brand new `#[cfg(test)] mod tests` block at the end of `book_bot/src/bots_manager/internal.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn commands_set_bot_ids_tracks_membership() {
        let bot_id = 900_101;
        assert!(!COMMANDS_SET_BOT_IDS.contains_key(&bot_id));

        COMMANDS_SET_BOT_IDS.insert(bot_id, ()).await;

        assert!(COMMANDS_SET_BOT_IDS.contains_key(&bot_id));
    }
}
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p book_bot --lib bots_manager 2>&1 | tail -30`
Expected: compile errors — `record_manager_fetch_failure` (mod.rs) and `COMMANDS_SET_BOT_IDS` (internal.rs) don't exist yet.

- [ ] **Step 3: Add the manager-fetch failure metric**

In `book_bot/src/bots_manager/mod.rs`, add just above `pub struct BotsManager;` (alongside the helpers added in Task 5):

```rust
fn record_manager_fetch_failure() {
    metrics::counter!("bots_manager_fetch_failures_total").increment(1);
}
```

Then replace, inside `async fn check`:

```rust
        let bots_data = match bots_data {
            Ok(v) => v,
            Err(err) => {
                log::info!("{err:?}");
                return;
            }
        };
```

with:

```rust
        let bots_data = match bots_data {
            Ok(v) => v,
            Err(err) => {
                log::error!("Failed to fetch bots from the manager API: {err:?}");
                record_manager_fetch_failure();
                return;
            }
        };
```

- [ ] **Step 4: Restructure the `join_next` drain loop**

In `book_bot/src/bots_manager/mod.rs`, replace:

```rust
        loop {
            if set_webhook_tasks.join_next().await.is_none() {
                break;
            }
        }
```

with:

```rust
        while let Some(res) = set_webhook_tasks.join_next().await {
            if let Err(join_err) = res {
                log::error!("set_webhook task panicked: {join_err:?}");
            }
        }
```

- [ ] **Step 5: Add `COMMANDS_SET_BOT_IDS` and gate `set_my_commands` behind it**

In `book_bot/src/bots_manager/mod.rs`, right after the existing line:

```rust
pub static INITED_BOTS_IDS: LazyLock<Cache<u32, ()>> = LazyLock::new(|| Cache::builder().build());
```

add:

```rust
pub static COMMANDS_SET_BOT_IDS: LazyLock<Cache<u32, ()>> =
    LazyLock::new(|| Cache::builder().build());
```

In `book_bot/src/bots_manager/internal.rs`, add `use crate::bots_manager::COMMANDS_SET_BOT_IDS;` next to the existing `use crate::bots_manager::BOTS_ROUTES;` import, then replace:

```rust
    let (handler, commands) = crate::bots::get_bot_handler();

    let set_command_result = match commands {
        Some(v) => bot.set_my_commands::<Vec<BotCommand>>(v).send().await,
        None => bot.delete_my_commands().send().await,
    };
    match set_command_result {
        Ok(_) => (),
        Err(err) => log::error!("{err:?}"),
    }
```

with:

```rust
    let (handler, commands) = crate::bots::get_bot_handler();

    if !COMMANDS_SET_BOT_IDS.contains_key(&bot_data.id) {
        let set_command_result = match commands {
            Some(v) => bot.set_my_commands::<Vec<BotCommand>>(v).send().await,
            None => bot.delete_my_commands().send().await,
        };

        match set_command_result {
            Ok(_) => {
                COMMANDS_SET_BOT_IDS.insert(bot_data.id, ()).await;
            }
            Err(err) => log::error!("{err:?}"),
        }
    }
```

- [ ] **Step 6: Run the tests to confirm they pass**

Run: `cargo test -p book_bot`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add book_bot/src/bots_manager/mod.rs book_bot/src/bots_manager/internal.rs
git commit -m "$(cat <<'EOF'
fix(bots_manager): log manager-fetch failures with a metric, set
commands once per bot, log JoinErrors from set_webhook tasks

Three independent minor fixes: manager unavailability now logs at
error level with a bots_manager_fetch_failures_total counter;
set_my_commands only runs once per bot id instead of on every TTI
wake-up; JoinErrors from the set_webhook JoinSet are logged instead of
silently discarded.
EOF
)"
```

---

## Task 7: Replace polling shutdown detection with tokio::signal + a watch channel

**Files:**
- Modify: `book_bot/src/main.rs`
- Modify: `book_bot/src/bots_manager/mod.rs` (`BotsManager::start`)
- Modify: `book_bot/src/bots_manager/axum_server.rs` (`start_axum_server`'s shutdown signal)
- Modify: `book_bot/Cargo.toml` (`tokio` features, remove `ctrlc`)
- Test: inline in `book_bot/src/bots_manager/mod.rs`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `BotsManager::start` now takes `tokio::sync::watch::Receiver<()>` instead of `Arc<AtomicBool>` — Task 9 (axum bind-before-spawn) depends on this same `watch::Receiver<()>` type being threaded through `start_axum_server`.

Today, Ctrl-C/SIGTERM sets an `AtomicBool` via the `ctrlc` crate, polled every second by both `BotsManager::start`'s loop and `start_axum_server`'s graceful-shutdown future — up to a full second of latency before shutdown is even noticed. Switch to a `tokio::sync::watch` channel fed by one `tokio::select!` on `tokio::signal::ctrl_c()`/SIGTERM, reacted to immediately via `.changed()`.

- [ ] **Step 1: Enable tokio's `signal` feature and remove `ctrlc`**

In `book_bot/Cargo.toml`, replace:

```toml
tokio = { version = "1.44.2", features = ["rt-multi-thread", "macros"] }
```

with:

```toml
tokio = { version = "1.44.2", features = ["rt-multi-thread", "macros", "signal"] }
```

and delete the line:

```toml
ctrlc = { version = "3.4.5", features = ["termination"] }
```

- [ ] **Step 2: Write the failing test for the tick-cadence helpers**

Append inside the `mod tests { ... }` block in `book_bot/src/bots_manager/mod.rs` (after the tests from Task 6):

```rust
    #[test]
    fn bots_data_check_runs_every_30_ticks_starting_at_zero() {
        assert!(BotsManager::should_run_bots_data_check(0));
        assert!(!BotsManager::should_run_bots_data_check(1));
        assert!(BotsManager::should_run_bots_data_check(30));
        assert!(BotsManager::should_run_bots_data_check(60));
    }

    #[test]
    fn pending_updates_check_runs_once_per_1800_tick_cycle() {
        assert!(!BotsManager::should_run_pending_updates_check(0));
        assert!(BotsManager::should_run_pending_updates_check(600));
        assert!(!BotsManager::should_run_pending_updates_check(601));
        assert!(!BotsManager::should_run_pending_updates_check(1799));
    }
```

- [ ] **Step 3: Run the test to confirm it fails**

Run: `cargo test -p book_bot --lib bots_manager::mod::tests::bots_data_check 2>&1 | tail -20`
Expected: compile error — `should_run_bots_data_check`/`should_run_pending_updates_check` don't exist yet.

- [ ] **Step 4: Replace `BotsManager::start`'s signature and loop**

In `book_bot/src/bots_manager/mod.rs`, change the top-of-file import:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
```

to:

```rust
use std::sync::Arc;
use tokio::sync::watch;
```

and change:

```rust
use tokio::time::{sleep, Duration};
```

to:

```rust
use tokio::time::{interval, sleep, Duration};
```

Then replace:

```rust
    pub async fn start(running: Arc<AtomicBool>) {
        BotsManager::wait_for_telegram_api().await;

        BotsManager::check(true).await;

        start_axum_server(running.clone()).await;

        let mut tick_number: i32 = 0;

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            if !running.load(Ordering::SeqCst) {
                BotsManager::stop_all().await;
                return;
            }

            if tick_number % 30 == 0 {
                BotsManager::check(false).await;
            }

            if tick_number % 1800 == 600 {
                BotsManager::check_pending_updates().await;
            }

            tick_number = (tick_number + 1) % 1800;
        }
    }
```

with:

```rust
    pub async fn start(mut shutdown_rx: watch::Receiver<()>) {
        BotsManager::wait_for_telegram_api().await;

        BotsManager::check(true).await;

        start_axum_server(shutdown_rx.clone()).await;

        let mut tick_number: i32 = 0;
        let mut ticker = interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    BotsManager::stop_all().await;
                    return;
                }
                _ = ticker.tick() => {}
            }

            if BotsManager::should_run_bots_data_check(tick_number) {
                BotsManager::check(false).await;
            }

            if BotsManager::should_run_pending_updates_check(tick_number) {
                BotsManager::check_pending_updates().await;
            }

            tick_number = (tick_number + 1) % 1800;
        }
    }

    fn should_run_bots_data_check(tick_number: i32) -> bool {
        tick_number % 30 == 0
    }

    fn should_run_pending_updates_check(tick_number: i32) -> bool {
        tick_number % 1800 == 600
    }
```

- [ ] **Step 5: Switch `start_axum_server` to the watch-based signal**

In `book_bot/src/bots_manager/axum_server.rs`, replace the import block:

```rust
use tokio::sync::Mutex;
use tokio::time;

use std::time::Duration;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
```

with:

```rust
use tokio::sync::{watch, Mutex};

use std::{net::SocketAddr, sync::Arc};
```

Then replace the function signature and its `tokio::spawn` body:

```rust
pub async fn start_axum_server(stop_signal: Arc<AtomicBool>) {
```

with:

```rust
pub async fn start_axum_server(mut shutdown_rx: watch::Receiver<()>) {
```

and replace:

```rust
    tokio::spawn(async move {
        log::info!("Start webserver...");

        let addr = SocketAddr::from(([0, 0, 0, 0], config::CONFIG.webhook_port));
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let mut interval = time::interval(Duration::from_secs(1));

                loop {
                    if !stop_signal.load(Ordering::SeqCst) {
                        break;
                    };

                    interval.tick().await;
                }
            })
            .await
            .unwrap();

        log::info!("Webserver shutdown...");
    });
```

with:

```rust
    tokio::spawn(async move {
        log::info!("Start webserver...");

        let addr = SocketAddr::from(([0, 0, 0, 0], config::CONFIG.webhook_port));
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            })
            .await
            .unwrap();

        log::info!("Webserver shutdown...");
    });
```

(Note the original `if !stop_signal.load(...) { break }` polling loop broke, i.e. triggered shutdown, as soon as `running` became `false` — the new `shutdown_rx.changed().await` fires the moment the sender sends, which is the same "start shutting down" trigger with no polling delay.)

- [ ] **Step 6: Replace `main.rs`'s Ctrl-C wiring**

Replace the full contents of `book_bot/src/main.rs`:

```rust
use std::str::FromStr;

use sentry::integrations::debug_images::DebugImagesIntegration;
use sentry::types::Dsn;
use sentry::ClientOptions;
use sentry_tracing::EventFilter;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;
use tracing::log;
use tracing_subscriber::filter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod bots;
mod bots_manager;
mod config;
pub mod handler_metrics;

#[tokio::main]
async fn main() {
    let _guard = if let Some(dsn_str) = &config::CONFIG.sentry_dsn {
        let dsn = Dsn::from_str(dsn_str).unwrap_or_else(|_| panic!("Cannot parse SENTRY_DSN"));
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

    let sentry_layer = sentry_tracing::layer().event_filter(|md| match md.level() {
        &tracing::Level::ERROR => EventFilter::Event,
        _ => EventFilter::Ignore,
    });

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(filter::LevelFilter::INFO)
        .with(sentry_layer)
        .init();

    let (shutdown_tx, shutdown_rx) = watch::channel(());

    tokio::spawn(async move {
        let mut sigterm =
            signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("Received SIGINT, shutting down...");
            }
            _ = sigterm.recv() => {
                log::info!("Received SIGTERM, shutting down...");
            }
        }

        let _ = shutdown_tx.send(());
    });

    bots_manager::BotsManager::start(shutdown_rx).await;
}
```

- [ ] **Step 7: Run the tick-cadence tests to confirm they pass**

Run: `cargo test -p book_bot --lib bots_manager::mod::tests`
Expected: PASS.

- [ ] **Step 8: Build and run the full test suite**

Run: `cargo build -p book_bot && cargo test -p book_bot`
Expected: both succeed. (`tokio::signal::unix` is Unix-specific; this project ships in a Linux container, matching the existing `ctrlc` "termination" feature's Unix-signal assumption.)

- [ ] **Step 9: Commit**

```bash
git add book_bot/Cargo.toml book_bot/src/main.rs book_bot/src/bots_manager/mod.rs book_bot/src/bots_manager/axum_server.rs
git commit -m "$(cat <<'EOF'
fix(bots_manager): replace polling shutdown signal with tokio::signal

Ctrl-C/SIGTERM previously set an AtomicBool polled every 1s in two
places. Switch to a tokio::sync::watch channel fed by tokio::signal,
reacted to immediately via .changed() instead of on the next tick.
Drops the ctrlc dependency.
EOF
)"
```

---

## Task 8: `stop_all` waits for dispatcher shutdown with a timeout instead of a blind sleep

**Files:**
- Modify: `book_bot/src/bots_manager/mod.rs` (`stop_all`)
- Test: inline in `book_bot/src/bots_manager/mod.rs`

**Interfaces:**
- Consumes: `StopTokenWithSender`'s `Arc<tokio::task::JoinHandle<()>>` field from Task 3; `watch`-based shutdown loop from Task 7 (which calls `stop_all()`).
- Produces: nothing consumed by later tasks.

`stop_all` today stops every bot's token, invalidates the routes, then blindly sleeps 5 seconds "and hopes" — long handlers (book downloads) get killed regardless of whether they were about to finish, and there's no signal of whether the wait was actually long enough.

- [ ] **Step 1: Write the failing tests**

Append inside the `mod tests { ... }` block in `book_bot/src/bots_manager/mod.rs` (after the tests from Task 7):

```rust
    #[tokio::test]
    async fn wait_for_handles_returns_zero_once_all_tasks_finish() {
        let handles = vec![
            Arc::new(tokio::spawn(async {})),
            Arc::new(tokio::spawn(async {})),
        ];

        let remaining = wait_for_handles(handles, Duration::from_secs(1)).await;
        assert_eq!(remaining, 0);
    }

    #[tokio::test]
    async fn wait_for_handles_gives_up_after_timeout() {
        let handles = vec![Arc::new(tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }))];

        let remaining = wait_for_handles(handles, Duration::from_millis(200)).await;
        assert_eq!(remaining, 1);
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

Run: `cargo test -p book_bot --lib bots_manager::mod::tests::wait_for_handles 2>&1 | tail -20`
Expected: compile error — `wait_for_handles` doesn't exist yet.

- [ ] **Step 3: Add the timeout-bounded polling helper and rewrite `stop_all`**

In `book_bot/src/bots_manager/mod.rs`, add just above `pub struct BotsManager;` (alongside the other free-function helpers):

```rust
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

async fn wait_for_handles(handles: Vec<Arc<tokio::task::JoinHandle<()>>>, timeout: Duration) -> usize {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = handles.iter().filter(|h| !h.is_finished()).count();
        if remaining == 0 || tokio::time::Instant::now() >= deadline {
            return remaining;
        }
        sleep(Duration::from_millis(100)).await;
    }
}
```

Then replace `stop_all`:

```rust
    pub async fn stop_all() {
        for (_, (stop_token, _, _, _)) in BOTS_ROUTES.iter() {
            stop_token.stop();
        }

        BOTS_ROUTES.invalidate_all();

        sleep(Duration::from_secs(5)).await;
    }
```

with:

```rust
    pub async fn stop_all() {
        let handles: Vec<Arc<tokio::task::JoinHandle<()>>> = BOTS_ROUTES
            .iter()
            .map(|(_, (stop_token, _, _, handle))| {
                stop_token.stop();
                handle
            })
            .collect();

        BOTS_ROUTES.invalidate_all();

        let total = handles.len();
        let remaining = wait_for_handles(handles, SHUTDOWN_TIMEOUT).await;

        if remaining == 0 {
            log::info!("All {total} bot dispatcher(s) shut down cleanly");
        } else {
            log::warn!(
                "Timed out after {}s waiting for {remaining}/{total} bot dispatcher(s) to shut down",
                SHUTDOWN_TIMEOUT.as_secs()
            );
        }
    }
```

- [ ] **Step 4: Run the tests to confirm they pass**

Run: `cargo test -p book_bot --lib bots_manager::mod::tests::wait_for_handles`
Expected: PASS. The timeout test should complete in ~200ms, not 60s.

- [ ] **Step 5: Run the full test suite**

Run: `cargo test -p book_bot`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add book_bot/src/bots_manager/mod.rs
git commit -m "$(cat <<'EOF'
fix(bots_manager): stop_all waits for dispatchers instead of a blind sleep

Replaced sleep(5)-and-hope with polling each dispatcher's JoinHandle
(is_finished, 100ms interval) up to a 10s deadline, logging how many
stragglers remained if the deadline was hit. Returns as soon as every
dispatcher has actually finished instead of always waiting the full
duration.
EOF
)"
```

---

## Task 9: Webhook server binds before spawning and its crash is detected by the main loop

**Files:**
- Modify: `book_bot/src/bots_manager/axum_server.rs` (`start_axum_server`)
- Modify: `book_bot/src/bots_manager/mod.rs` (`BotsManager::start`)
- Test: inline in `book_bot/src/bots_manager/axum_server.rs`

**Interfaces:**
- Consumes: `watch::Receiver<()>` shutdown signal from Task 7.
- Produces: nothing consumed by later tasks (final task in this plan).

Today `TcpListener::bind` and `axum::serve` both happen inside the spawned task with `.unwrap()`; if the port is taken, the panic is buried in a detached task and (relying on `panic = "abort"`) either aborts the whole process abruptly or, without that reliance, would leave the process alive with no server. Move `bind` before the spawn so a conflict fails fast with a clear log line and a controlled `std::process::exit(1)`; return the `serve` task's `JoinHandle` so the main loop can detect an unexpected exit later.

- [ ] **Step 1: Write the failing test**

Append to `book_bot/src/bots_manager/axum_server.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_webhook_listener_fails_when_port_already_taken() {
        let first = bind_webhook_listener(0).await.unwrap();
        let port = first.local_addr().unwrap().port();

        let second = bind_webhook_listener(port).await;

        assert!(second.is_err());
    }
}
```

- [ ] **Step 2: Run the test to confirm it fails**

Run: `cargo test -p book_bot --lib axum_server 2>&1 | tail -20`
Expected: compile error — `bind_webhook_listener` doesn't exist yet.

- [ ] **Step 3: Extract the bind step, return a `Result<JoinHandle<()>>`**

In `book_bot/src/bots_manager/axum_server.rs`, add this function above `pub async fn start_axum_server`:

```rust
async fn bind_webhook_listener(port: u16) -> std::io::Result<tokio::net::TcpListener> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tokio::net::TcpListener::bind(addr).await
}
```

Then change the signature:

```rust
pub async fn start_axum_server(mut shutdown_rx: watch::Receiver<()>) {
```

to:

```rust
pub async fn start_axum_server(
    mut shutdown_rx: watch::Receiver<()>,
) -> std::io::Result<tokio::task::JoinHandle<()>> {
```

and replace the tail of the function:

```rust
    tokio::spawn(async move {
        log::info!("Start webserver...");

        let addr = SocketAddr::from(([0, 0, 0, 0], config::CONFIG.webhook_port));
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            })
            .await
            .unwrap();

        log::info!("Webserver shutdown...");
    });
}
```

with:

```rust
    let listener = bind_webhook_listener(config::CONFIG.webhook_port).await?;
    log::info!(
        "Webhook server listening on port {}",
        config::CONFIG.webhook_port
    );

    let handle = tokio::spawn(async move {
        log::info!("Start webserver...");

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            })
            .await
            .unwrap();

        log::info!("Webserver shutdown...");
    });

    Ok(handle)
}
```

- [ ] **Step 4: Handle the new `Result` and monitor the handle in `BotsManager::start`**

In `book_bot/src/bots_manager/mod.rs`, replace:

```rust
        start_axum_server(shutdown_rx.clone()).await;

        let mut tick_number: i32 = 0;
        let mut ticker = interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    BotsManager::stop_all().await;
                    return;
                }
                _ = ticker.tick() => {}
            }

            if BotsManager::should_run_bots_data_check(tick_number) {
```

with:

```rust
        let server_handle = match start_axum_server(shutdown_rx.clone()).await {
            Ok(handle) => handle,
            Err(err) => {
                log::error!("Failed to start webhook server: {err}");
                std::process::exit(1);
            }
        };

        let mut tick_number: i32 = 0;
        let mut ticker = interval(Duration::from_secs(1));

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    BotsManager::stop_all().await;
                    return;
                }
                _ = ticker.tick() => {}
            }

            if server_handle.is_finished() {
                log::error!("Webhook server task exited unexpectedly; shutting down");
                std::process::exit(1);
            }

            if BotsManager::should_run_bots_data_check(tick_number) {
```

- [ ] **Step 5: Run the new test to confirm it passes**

Run: `cargo test -p book_bot --lib axum_server`
Expected: PASS.

- [ ] **Step 6: Run the full test suite and build**

Run: `cargo build -p book_bot && cargo test -p book_bot`
Expected: both succeed.

- [ ] **Step 7: Manual verification (SIGTERM behavior, per the spec's acceptance criteria)**

This step is integration-shaped and not covered by the automated suite:

Run: `cargo run -p book_bot` (with valid env vars configured), in another terminal send `kill -TERM <pid>`, and confirm the logs show "Received SIGTERM, shutting down..." followed by either "All N bot dispatcher(s) shut down cleanly" or a timeout warning, and the process exits — not a silent hang past `SHUTDOWN_TIMEOUT` (10s).

- [ ] **Step 8: Commit**

```bash
git add book_bot/src/bots_manager/axum_server.rs book_bot/src/bots_manager/mod.rs
git commit -m "$(cat <<'EOF'
fix(bots_manager): bind webhook server before spawning, monitor its JoinHandle

bind()+serve() previously both ran unwrap()'d inside a detached spawned
task. Bind happens first now: a taken port fails fast with a clear log
line and a controlled exit(1) before the main loop ever starts. The
serve task's JoinHandle is monitored each tick so an unexpected server
exit also terminates the process instead of running headless.
EOF
)"
```

---

## Final verification

- [ ] Run `cargo test -p book_bot` — all tests pass.
- [ ] Run `cargo clippy -p book_bot -- -D warnings` — no new warnings.
- [ ] Run `cargo build -p book_bot --release` — builds cleanly (confirms nothing depends on `dev-dependencies` leaking into the release binary, and that `panic = "abort"` still links).
- [ ] Re-read `docs/superpowers/specs/2026-07-08-bots-manager-lifecycle-reliability-design.md`'s Acceptance criteria and confirm each one is covered:
  - Taken port → fails at startup with a clear error: Task 9.
  - Bot deletion / `BotCache` change picked up within one cycle: Task 4.
  - SIGTERM waits for handlers with a timeout: Tasks 7 + 8.
  - Three consecutive failures trip the breaker, success resets it: Task 5.
