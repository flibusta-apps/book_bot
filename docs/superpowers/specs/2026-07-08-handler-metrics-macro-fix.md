# Spec 03: Handler metrics correctness — `?` bypasses error accounting in the `log_handler` macro

- **Priority:** high
- **Effort:** S
- **Category:** observability / correctness

## Problem

### 3.1. The `log_handler` macro does not see errors propagated via `?`

`book_bot_macros/src/lib.rs:17-28` expands to:
```rust
let mut __metrics_guard = crate::handler_metrics::HandlerMetricsGuard::new(#handler_name);
let __result: anyhow::Result<()> = #fn_block;
if __result.is_err() {
    __metrics_guard.set_error();
}
__result
```
`#fn_block` is inserted as a block expression **inside the original function**, so the `?` operator (and `return Err(...)`) in the handler body returns from the whole function, bypassing `if __result.is_err()`. The guard is dropped with `success: true` (`book_bot/src/handler_metrics.rs:16`), and the error is counted as a success.

Handlers use `?` pervasively (e.g. ~20 occurrences in `modules/search/mod.rs`), so the `handler_requests_total{status="error"}` metric is systematically undercounted — effectively only errors returned by the final expression are counted.

### 3.2. Some handlers additionally mask errors as `Ok(())`

- `modules/search/mod.rs:178-181, 209-212, 240-243, 271-274` — after sending `ERROR_TRY_LATER` to the user, `Ok(())` is returned instead of `Err(err)`;
- `modules/download/mod.rs:593` — `let _ = wait_archive(...)` discards the result.

Even after the macro fix, these errors will not reach metrics or logs.

## Proposed solution

1. In the macro, wrap the body in an async block so `?` cannot escape the wrapper (the body is already an `async fn`, so semantics are preserved):
   ```rust
   let __result: anyhow::Result<()> = async #fn_block.await;
   ```
2. In handlers: after sending the error message to the user, return `Err(err)` (following the `generic_search_pagination_handler` pattern); replace `let _ = wait_archive(...)` with logging of the result.
3. Add a test for the macro (see Spec 09 on tests): a handler failing via `?` must increment the error metric. The `book_bot_macros` crate currently has zero tests — at minimum a snapshot test of the expansion (`trybuild` or `TokenStream` comparison).

## Acceptance criteria

- A handler returning `Err` via `?` on its first line is reflected in `handler_requests_total{status="error"}`.
- `cargo test -p book_bot_macros` contains at least one test of the macro expansion.
