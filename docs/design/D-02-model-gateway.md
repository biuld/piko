# D-02: Model-gateway retry budget and streaming fallback

> Status: implemented
> Implements: [F-02](../features/F-02-model-gateway.md)
> Decisions: product decisions live in the F-02 PRD (per-request budget, capped
> jittered backoff, per-provider fallback opt-out, no mid-stream restart,
> streaming usage capture)

## Goal

Close the F-02 M0 gaps in `piko-llmd`:

- **A. Retry/backoff budget** — retries are bounded by attempts *and* total
  retry time; backoff is capped and jittered; retryable errors are classified
  structurally instead of by string matching alone.
- **B. Open-phase status inspection** — genai defers HTTP status checks to the
  first stream polls, so 503/429/401 errors surface as stream events; the open
  phase peeks those events and gives status failures the bounded retry
  treatment.
- **C. Streaming fallback** — after the retry budget is exhausted on
  retryable failures, one non-streaming completion per request, controllable
  per provider, emitted as the standard event sequence.
- **D. Streaming usage capture** — streaming requests ask providers to report
  usage so completed turns carry token/cost metadata.

## Constraints and non-goals

- The gateway stays stateless per request: no session transport state, no
  prewarm, no sticky routing (later F-02 slice).
- The frozen `GatewayRequest` is immutable during a request; a retry reuses
  the same request, never re-assembles the prompt.
- No mid-stream restart: callers (orchd) consume deltas incrementally and
  commit the assistant message at step end, so a gateway-side restart after
  content delivery would duplicate or corrupt committed text. Mid-stream
  failures keep the existing `GatewayEvent::Error` semantics; step-level retry
  is the recovery path.
- Fallback emits the same event contract as streaming; no new event kinds.
- Middleware `pre_chat` hooks run once per request, not once per attempt.
- Backward compatibility: existing settings files without the new keys parse
  with defaults.

## Proposed design

### A. Retry/backoff budget (`piko-protocol` + hostd settings)

`RetryConfig` gains two fields (both with serde defaults, so existing
settings/profiles keep working):

```rust
pub struct RetryConfig {
    pub enabled: bool,          // default true
    pub max_retries: u32,       // default 3
    pub base_delay_ms: u64,     // default 2000
    pub max_delay_ms: u64,      // default 30_000 (new; per-attempt cap)
    pub budget_ms: u64,         // default 60_000 (new; total retry time)
}
```

`ProviderConfig` gains `streaming_fallback: Option<bool>` (`streamingFallback`,
default `true` when absent) so fallback is a per-provider opt-out.

Hostd `RetrySettings` (`[retry]` in `settings.toml`) gains the same two
optional keys, threaded through `merge_retry` and the `orch_factory` wiring;
`default_settings()` sets 30_000 / 60_000.

### B. `piko-llmd` retry module

New `packages/llmd/src/retry.rs` (pure, unit-testable):

- `next_delay_ms(base_delay_ms, max_delay_ms, retry_index, jitter)` —
  `min(base × 2^(index−1), max) × jitter`, index is 1-based.
- `RetryPolicy::from_config(&RetryConfig)` holding enabled/max/base/cap/budget.
- `RetryPolicy::delay_for_retry(retries_used, elapsed_ms, jitter) -> Option<u64>`
  — `None` when disabled, attempts exhausted, or `elapsed + delay > budget`.
- `is_retryable(&genai::Error) -> bool` — structural classification:
  - `HttpError { status, .. }` → retryable status set;
  - `WebModelCall`/`WebAdapterCall` → `webc::Error::ResponseFailedStatus`
    status, or `webc::Error::Reqwest` connect/timeout;
  - `WebStream` → downcast the wrapped error (`genai::Error` for deferred
    status checks, `reqwest::Error` for connect/timeout); unknown stream
    breaks are retryable;
  - `StreamParse` → retryable;
  - otherwise fall back to the existing string classifier on `to_string()`.
- `RetryState { retries_used, elapsed_ms }` accumulates across attempts.

### C. Open phase with status peeking (`stream.rs`)

`open_stream_with_retry` opens the stream and then polls the synthesized
`Start` event plus the next event (`peek_open_events`). genai always
synthesizes `Start` first and defers the HTTP status check to the second poll,
so this peek catches 503/429/401 etc. as open-phase failures. The first real
event is re-injected with `futures::stream::iter([first]).chain(rest)`, so no
content is lost.

Open failures are classified:

```text
NonRetryable(msg)       -> chat_stream returns Err immediately
                           (auth, bad request, cancelled)
BudgetExhausted(msg)    -> if provider.streaming_fallback:
                               one exec_chat with the same request+options
                               -> FallbackEvents(standard event sequence)
                               -> Err on failure
                           else: return Err(msg)
```

Backoff sleep is cancellable (`tokio::select!` on the token), and retries stop
when the next delay would exceed the remaining budget.

`resilient_stream` then maps provider chunks 1:1 to gateway events with the
existing middleware chain (token usage + cost annotate `Usage`). A mid-stream
`Err` or premature EOF yields `GatewayEvent::Error` and terminates without
`Done`.

### D. Streaming usage capture

`chat_stream` builds `ChatOptions` with `with_capture_usage(true)`, so
streaming responses report usage when the provider supports it (OpenAI-family
requests gain `stream_options.include_usage`; Anthropic/Gemini parse usage
from their own stream events). `fallback_events` derives usage from the
non-streaming `ChatResponse`.

`llm_call` gets the same open-phase retry wrapper (already non-streaming, so no
fallback and no peeking).

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | `RetryConfig` + `max_delay_ms`/`budget_ms`; `ProviderConfig` + `streaming_fallback` |
| `piko-hostd` | `RetrySettings` + two keys; `merge_retry`; `default_settings`; `orch_factory` wiring |
| `piko-llmd` | new `retry.rs`; new `stream.rs` (peek, open retry, fallback events, consume); `executor.rs` eager open + fallback + retried `llm_call`; per-provider fallback lookup |
| `piko-orchd` | none (gateway port unchanged) |
| `piko-sandbox` | none |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Retry budget exhausted in the open phase returns `Err` with the last error
  (logs keep the attempt history); fallback fires only for retryable
  exhaustion.
- Non-retryable open failures (401/400/422) and cancellation fail immediately
  with no retries and no fallback.
- Mid-stream failure or premature EOF yields `GatewayEvent::Error` and
  terminates without `Done`; no silent restart.
- Cancellation during backoff sleep uses `tokio::select!` on the token, so the
  request fails closed immediately instead of waiting out the delay.

## Verification

- Unit tests in `retry.rs`: backoff values, cap, jitter range; budget
  exhaustion at the exact boundary; disabled retries; status-code
  classification (429/503 retryable, 400/401/422 not).
- Integration tests in `packages/llmd/tests/gateway_retry.rs` against a local
  stub HTTP server (tokio `TcpListener`, no external network):
  - streaming returns 503 twice then success: `chat_stream` succeeds after
    retries and the server saw the expected attempt count;
  - streaming always 503 + fallback enabled: response arrives as
    `ContentDelta`/`Usage`/`Done` from the non-streaming endpoint;
  - streaming always 503 + fallback disabled: `chat_stream` fails after the
    retry budget with no non-streaming request;
  - a stream that emits a chunk then closes mid-way surfaces an error with no
    second request (no silent restart);
  - a 401 on open fails immediately with no retries and no fallback;
  - `llm_call` retries a transient 503 then succeeds non-streaming.
- Differential reference: codex-rs `responses_retry.rs` retry-loop semantics
  and `client.rs` fallback activation.
- `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`.

## Alternatives considered

- String-only retry classification: rejected — status codes are the
  authoritative signal and are available structurally in genai errors
  (including wrapped inside `WebStream`); the string matcher stays only as
  fallback.
- Unbounded budget (attempts only): rejected — the PRD requires a bounded
  worst-case wait, and the roadmap calls for a budget explicitly.
- Mid-stream restart after partial content: rejected — callers commit deltas
  incrementally, so discarding partial content would require a reset protocol
  that leaks partial deltas into realtime and risks transcript corruption;
  step-level retry is the recovery path.
- Fallback always on with no provider opt-out: rejected — providers differ in
  non-streaming support; the digest lists per-provider fallback as the gap.
- Fallback on non-retryable failures: rejected — an auth failure would repeat
  the same auth failure and waste a request.

## Rollout

1. `piko-protocol` + hostd settings: budget/cap/fallback fields and wiring.
2. `piko-llmd` `retry.rs`: policy, backoff, budget, classification + unit tests.
3. `piko-llmd` `stream.rs`: status peeking, open retry with budget, fallback
   events, consume loop; executor wiring; `llm_call` retry.
4. Stub-server integration tests for retry, fallback, fallback opt-out,
   mid-stream error surfacing, and non-retryable short-circuit.
5. V-02 evidence and status updates (feature index, roadmap, digest).
