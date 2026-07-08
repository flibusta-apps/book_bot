# Handler Metrics Macro Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `handler_requests_total{status="error"}` correctly count handler errors that propagate via `?`, and stop the handlers that were silently converting errors into `Ok(())`.

**Architecture:** The `#[log_handler]` proc-macro in `book_bot_macros` currently splices the handler's body in as `let __result: anyhow::Result<()> = #fn_block;`. Because `#fn_block` is inlined directly into the wrapper function, a `?` inside the handler body returns from the *whole function*, skipping the `if __result.is_err() { __metrics_guard.set_error(); }` check entirely — so the `HandlerMetricsGuard` (`book_bot/src/handler_metrics.rs`) always drops with `success: true`. The fix wraps the body in its own `async { ... }.await` block so `?` only escapes that inner block, leaving `__result` correctly populated before the guard check runs. Separately, four branches in `modules/search/mod.rs` and one call site in `modules/download/mod.rs` swallow real errors as `Ok(())` (or discard them via `let _ = ...`), which would still hide errors from metrics/logs even after the macro fix — those are fixed to propagate/log the error.

**Tech Stack:** Rust, `syn`/`quote`/`proc-macro2` (proc-macro authoring), `metrics` 0.24 (custom `Recorder` test double via `with_local_recorder`), `futures::executor::block_on` for driving an async handler synchronously in a test.

## Global Constraints

- No new crate dependencies. Macro-expansion testing uses a `TokenStream` comparison built from `syn`/`quote`/`proc-macro2` (already dependencies of `book_bot_macros`) — not `trybuild`.
- Metric-behavior testing uses a hand-written `metrics::Recorder` test double plus `metrics::with_local_recorder` and `futures::executor::block_on` (both already available via existing `metrics` and `futures` dependencies of `book_bot`) — not `metrics-util`.
- `book_bot` has no `tests/` integration-test directory (it's a binary-only crate, no `[lib]` target) — all tests are `#[cfg(test)] mod tests` blocks inside `src/*.rs`, matching existing convention (e.g. `book_bot/src/bots/approved_bot/services/rate_limit.rs`).
- Preserve existing macro behavior for the log statement (`generate_log_stmt`) and for handlers that already return `Err` from their final expression — only the `?`-escape path is broken today.

---

## Task 1: Regression test proving `?` bypasses the error metric

**Files:**
- Modify: `book_bot/src/handler_metrics.rs`

**Interfaces:**
- Consumes: `crate::handler_metrics::HandlerMetricsGuard` (existing, unchanged), `book_bot_macros::log_handler` (existing, about to be fixed in Task 2).
- Produces: nothing consumed by later tasks — this is a standalone regression test that Task 2 must turn green.

This test is written against the **current, still-buggy** macro. It must fail now and pass once Task 2's fix lands.

- [ ] **Step 1: Append the regression test to `book_bot/src/handler_metrics.rs`**

Add this at the end of the file (after the existing `impl Drop for HandlerMetricsGuard` block):

```rust
#[cfg(test)]
mod tests {
    use book_bot_macros::log_handler;
    use metrics::{
        Counter, CounterFn, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct ErrorCountingRecorder {
        error_increments: Arc<Mutex<u64>>,
    }

    struct ErrorCounter(Arc<Mutex<u64>>);

    impl CounterFn for ErrorCounter {
        fn increment(&self, value: u64) {
            *self.0.lock().unwrap() += value;
        }

        fn absolute(&self, value: u64) {
            *self.0.lock().unwrap() = value;
        }
    }

    impl Recorder for ErrorCountingRecorder {
        fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
        fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
        fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

        fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
            let is_error_counter = key.name() == "handler_requests_total"
                && key.labels().any(|l| l.key() == "status" && l.value() == "error");

            if is_error_counter {
                Counter::from_arc(Arc::new(ErrorCounter(self.error_increments.clone())))
            } else {
                Counter::noop()
            }
        }

        fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge {
            Gauge::noop()
        }

        fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
            Histogram::noop()
        }
    }

    fn fails() -> anyhow::Result<()> {
        Err(anyhow::anyhow!("boom"))
    }

    #[log_handler("macro_error_propagation_test")]
    async fn fails_via_question_mark() -> anyhow::Result<()> {
        fails()?;
        Ok(())
    }

    #[test]
    fn question_mark_error_increments_error_metric() {
        let recorder = ErrorCountingRecorder::default();
        let error_increments = recorder.error_increments.clone();

        metrics::with_local_recorder(&recorder, || {
            let result = futures::executor::block_on(fails_via_question_mark());
            assert!(result.is_err());
        });

        assert_eq!(*error_increments.lock().unwrap(), 1);
    }
}
```

- [ ] **Step 2: Run the test and confirm it fails against the current (buggy) macro**

Run: `cargo test -p book_bot question_mark_error_increments_error_metric -- --nocapture`

Expected: FAIL with `assertion \`left == right\` failed` — left is `0`, because `fails()?` returns from `fails_via_question_mark` before `__metrics_guard.set_error()` is ever called, so the guard drops with `success: true` and no `status="error"` counter is ever registered.

- [ ] **Step 3: Commit**

```bash
git add book_bot/src/handler_metrics.rs
git commit -m "test: add failing regression test for ?-bypassed handler error metrics"
```

---

## Task 2: Fix the macro so `?` cannot bypass the metrics guard

**Files:**
- Modify: `book_bot_macros/src/lib.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: `expand_log_handler(handler_name: &syn::LitStr, input_fn: &syn::ItemFn) -> proc_macro2::TokenStream` — a pure function extracted from the `#[proc_macro_attribute]` entry point so it can be exercised directly from a unit test (raw `proc_macro::TokenStream` cannot be constructed outside of an actual macro invocation).

- [ ] **Step 1: Refactor `log_handler` into a testable pure function, and add a failing snapshot test for the fix**

Replace the full contents of `book_bot_macros/src/lib.rs` with:

```rust
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, LitStr, Pat, PatType, Type, TypePath};

#[proc_macro_attribute]
pub fn log_handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let handler_name = parse_macro_input!(attr as LitStr);
    let input_fn = parse_macro_input!(item as ItemFn);

    expand_log_handler(&handler_name, &input_fn).into()
}

fn expand_log_handler(handler_name: &LitStr, input_fn: &ItemFn) -> TokenStream2 {
    let fn_vis = &input_fn.vis;
    let fn_sig = &input_fn.sig;
    let fn_block = &input_fn.block;
    let fn_attrs = &input_fn.attrs;

    let log_stmt = generate_log_stmt(input_fn, handler_name);

    quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            #log_stmt
            let mut __metrics_guard = crate::handler_metrics::HandlerMetricsGuard::new(#handler_name);
            let __result: anyhow::Result<()> = #fn_block;
            if __result.is_err() {
                __metrics_guard.set_error();
            }
            __result
        }
    }
}

fn get_type_ident(ty: &Type) -> Option<String> {
    if let Type::Path(TypePath { path, .. }) = ty {
        path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

fn get_param_ident(fn_arg: &FnArg) -> Option<(proc_macro2::Ident, String)> {
    if let FnArg::Typed(PatType { pat, ty, .. }) = fn_arg {
        if let Pat::Ident(pat_ident) = pat.as_ref() {
            if let Some(type_name) = get_type_ident(ty) {
                return Some((pat_ident.ident.clone(), type_name));
            }
        }
    }
    None
}

fn generate_log_stmt(input_fn: &ItemFn, handler_name: &LitStr) -> proc_macro2::TokenStream {
    for fn_arg in &input_fn.sig.inputs {
        if let Some((ident, type_name)) = get_param_ident(fn_arg) {
            match type_name.as_str() {
                "Message" => {
                    return quote! {
                        tracing::info!(
                            handler = #handler_name,
                            user_id = ?#ident.from.as_ref().map(|u| u.id.0)
                        );
                    };
                }
                "CallbackQuery" => {
                    return quote! {
                        tracing::info!(
                            handler = #handler_name,
                            user_id = #ident.from.id.0
                        );
                    };
                }
                _ => continue,
            }
        }
    }

    // Fallback: log without user_id if no known type found
    quote! {
        tracing::info!(handler = #handler_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    #[test]
    fn wraps_handler_body_in_async_block_so_question_mark_is_captured() {
        let handler_name: LitStr = parse_str(r#""test_handler""#).unwrap();
        let input_fn: ItemFn = parse_str(
            r#"
            async fn my_handler() -> anyhow::Result<()> {
                fails()?;
                Ok(())
            }
            "#,
        )
        .unwrap();

        let expanded = expand_log_handler(&handler_name, &input_fn).to_string();

        let expected = quote! {
            async fn my_handler() -> anyhow::Result<()> {
                tracing::info!(handler = #handler_name);
                let mut __metrics_guard = crate::handler_metrics::HandlerMetricsGuard::new(#handler_name);
                let __result: anyhow::Result<()> = async {
                    fails()?;
                    Ok(())
                }.await;
                if __result.is_err() {
                    __metrics_guard.set_error();
                }
                __result
            }
        }
        .to_string();

        assert_eq!(expanded, expected);
    }
}
```

Note this step intentionally keeps the buggy line `let __result: anyhow::Result<()> = #fn_block;` — the test above is written for the *fixed* expansion and must fail against this line, confirming the test actually exercises the bug.

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test -p book_bot_macros wraps_handler_body_in_async_block_so_question_mark_is_captured -- --nocapture`

Expected: FAIL with an `assertion \`left == right\` failed` where the `left` (actual) string contains `let __result : anyhow :: Result < () > = { fails () ? ; Ok (()) } ;` instead of the expected `async { fails () ? ; Ok (()) } . await`.

- [ ] **Step 3: Apply the fix**

In `book_bot_macros/src/lib.rs`, inside `expand_log_handler`, change:

```rust
            let __result: anyhow::Result<()> = #fn_block;
```

to:

```rust
            let __result: anyhow::Result<()> = async #fn_block.await;
```

- [ ] **Step 4: Run both the macro test and Task 1's regression test, confirm both pass**

Run: `cargo test -p book_bot_macros -p book_bot`

Expected: PASS for `wraps_handler_body_in_async_block_so_question_mark_is_captured` and for `question_mark_error_increments_error_metric` (from Task 1).

- [ ] **Step 5: Commit**

```bash
git add book_bot_macros/src/lib.rs
git commit -m "fix(macros): wrap log_handler body in async block so ? can't bypass the metrics guard"
```

---

## Task 3: Stop `modules/search/mod.rs` from masking errors as `Ok(())`

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/search/mod.rs:178-181,209-212,240-243,271-274`

**Interfaces:**
- Consumes: the fixed `#[log_handler]` macro from Task 2 (so the propagated `Err` is now correctly counted).
- Produces: nothing consumed by later tasks.

All four branches (`Book`, `Authors`, `Sequences`, `Translators` in `message_handler`) have the identical shape:

```rust
                        Err(_) => {
                            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                            return Ok(());
                        }
```

- [ ] **Step 1: Replace all four occurrences**

In `book_bot/src/bots/approved_bot/modules/search/mod.rs`, replace every occurrence of:

```rust
                        Err(_) => {
                            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                            return Ok(());
                        }
```

with:

```rust
                        Err(err) => {
                            safe_send_message(&bot, chat_id, ERROR_TRY_LATER, None).await?;
                            return Err(err);
                        }
```

(This is the same replacement applied 4 times — once per `SearchCallbackData` branch — and mirrors the existing pattern already used in `generic_search_pagination_handler` in the same file, e.g. lines 97-101.)

- [ ] **Step 2: Verify the crate still compiles**

Run: `cargo build -p book_bot`

Expected: builds with no new warnings or errors. (`err` is now used, so there should be no "unused variable" warning; `search_book`/`search_author`/`search_sequence`/`search_translator` all return `anyhow::Result<Option<Page<_, _>>>`, matching the `BotHandlerInternal` return type of `message_handler`.)

- [ ] **Step 3: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/search/mod.rs
git commit -m "fix(search): propagate search errors instead of masking them as Ok(())"
```

---

## Task 4: Stop `modules/download/mod.rs` from discarding `wait_archive`'s result

**Files:**
- Modify: `book_bot/src/bots/approved_bot/modules/download/mod.rs:596`

**Interfaces:**
- Consumes: nothing new.
- Produces: nothing consumed by later tasks.

`download_archive` (decorated with `#[log_handler("download")]`) currently does:

```rust
    let _ = wait_archive(bot, task.id, message).await;

    Ok(())
```

`wait_archive` polls the archive task in a loop and can return `Err` (e.g. `get_task` failing) — that error is silently dropped and `download_archive` always reports success.

- [ ] **Step 1: Replace the discard with error logging**

In `book_bot/src/bots/approved_bot/modules/download/mod.rs`, change:

```rust
    let _ = wait_archive(bot, task.id, message).await;

    Ok(())
```

to:

```rust
    if let Err(err) = wait_archive(bot, task.id, message).await {
        log::error!("{err:?}");
    }

    Ok(())
```

This matches the existing `log::error!("{err:?}");` pattern already used earlier in the same function (line 582) and in `wait_archive` itself (line 418). `wait_archive` already sends the user an error message via `send_error_message` before returning `Err`, so no additional user-facing message is needed here — only the logging was missing.

- [ ] **Step 2: Verify the crate still compiles**

Run: `cargo build -p book_bot`

Expected: builds with no new warnings or errors.

- [ ] **Step 3: Commit**

```bash
git add book_bot/src/bots/approved_bot/modules/download/mod.rs
git commit -m "fix(download): log wait_archive errors instead of discarding them"
```

---

## Task 5: Full workspace verification

**Files:** none (verification only)

**Interfaces:**
- Consumes: all prior tasks' changes.
- Produces: nothing.

- [ ] **Step 1: Run the full test suite**

Run: `cargo test --workspace`

Expected: all tests pass, including `question_mark_error_increments_error_metric` (Task 1) and `wraps_handler_body_in_async_block_so_question_mark_is_captured` (Task 2).

- [ ] **Step 2: Run clippy across the workspace**

Run: `cargo clippy --workspace --all-features`

Expected: no new warnings introduced by this change (pre-existing warnings, if any, are out of scope).

- [ ] **Step 3: Build the full workspace in release mode**

Run: `cargo build --workspace --release`

Expected: builds successfully.
