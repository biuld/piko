# V-02: F-02 model-gateway slice acceptance evidence

> Date: 2026-08-02
> Fixture: `piko-llmd` `retry.rs` unit tests and `tests/gateway_retry.rs`
> stub-server integration tests (local `TcpListener`, no external network)
> Environment: macOS, `cargo test -p piko-llmd`, `cargo test --workspace`

## Reproduction

```bash
cargo test -p piko-llmd --lib retry
cargo test -p piko-llmd --test gateway_retry
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The integration tests point piko's native HTTP adapters at a scripted stub
server that serves HTTP status responses, SSE streams, and protocol JSON.

## Result

All F-02 acceptance criteria for the M0 slice pass:

- **Backoff/budget**: exponential delays (base 2s → 4s → 8s) with a 30 s cap;
  jitter within `[0.9, 1.1]`; retries stop at `max_retries` or when the next
  delay exceeds the remaining budget (2 s budget boundary verified); disabled
  retries never schedule.
- **Classification**: 408/409/425/429/500/502/503/504 are retryable;
  400/401/403/404/422 fail fast through piko-owned typed errors.
- **Retry end-to-end**: a streaming endpoint that returns 503 twice then
  succeeds completes with content deltas, a `Usage` event (input 3 / output 2),
  and `Done("stop")` after exactly three attempts.
- **Streaming fallback**: three 503 attempts then one non-streaming request
  yields `TextDelta("fallback text")`, `Usage` (input 10 / output 5), and
  `Completed("stop")`; exactly one non-streaming request is made.
- **Per-provider opt-out**: with `streamingFallback: false`, the request fails
  after the retry budget with no non-streaming request.
- **Mid-stream break**: a stream that emits a chunk then closes surfaces
  `ModelEvent::Error` with no second request — no silent restart, no
  duplicated response.
- **Non-retryable short-circuit**: a 401 fails immediately with one request,
  no retries, no fallback.
- **`llm_call`**: a transient 503 then a non-streaming success completes with
  the response text (two requests).

## Invariants

- Retry time is bounded per request by `budget_ms` and per attempt by
  `max_delay_ms`; cancellation during backoff aborts immediately.
- Fallback fires only after the budget is exhausted on retryable failures —
  never on auth/bad-request failures or cancellation.
- The gateway never restarts a stream after content has been delivered;
  mid-stream failures surface as errors and callers own recovery.
- Every completed response emits a `Usage` event before completion when the
  provider reports usage through the selected protocol's native controls.
- Existing settings files without the new keys parse with defaults
  (`max_delay_ms` 30_000, `budget_ms` 60_000, `streamingFallback` enabled).
- `cargo clippy --workspace --all-targets -- -D warnings` is clean and
  `cargo test --workspace` passes.
