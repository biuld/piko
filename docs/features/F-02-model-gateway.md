# F-02: Model gateway

> Status: implemented
> Priority: P0
> Source evidence: codex-rs `core/src/client.rs`, `core/src/client_common.rs`,
> `core/src/responses_retry.rs`, `core/src/util.rs`,
> `core/src/session_startup_prewarm.rs`

## Summary

A model step is executed through a **model gateway**: a single port that turns
an agent-runtime request (provider, model, frozen prompt, transcript, tools,
thinking level) into a stream of typed events, transparently applying retry
and backoff within a bounded budget, and falling back to a non-streaming
request when streaming cannot be established within that budget. The gateway
also carries usage and cost metadata with every completed response so the
runtime can account for tokens without knowing provider details.

## Problem

Agent runs depend on providers that fail transiently: connections drop, rate
limits trigger, and streaming endpoints can be flaky. Without a gateway
contract, each call site would have to decide what is retryable, how long to
wait, and what to do when streaming fails — and retry logic that has no time
budget can stall a turn for an unbounded amount of time. piko's gateway
already streams events, tracks usage, and retries a handful of transient
errors, but three behaviors are underspecified:

1. **Retry/backoff budget.** Retries are bounded only by an attempt count.
   There is no cap on per-attempt delay, no jitter, and no total time budget,
   so a burst of rate-limit errors can block a turn far beyond what the user
   configured. HTTP status failures that the transport defers to the stream
   must still receive the same bounded retry treatment.
2. **Streaming fallback.** When streaming cannot be established within the
   retry budget, the request fails even though a non-streaming completion
   would succeed. Providers have different streaming reliability, so fallback
   must be controllable per provider.
3. **Usage metadata.** Streaming responses only carry usage when the provider
   reports it and the gateway asks for it; without capture enabled, completed
   turns miss token/cost accounting.

## User journeys

1. A user sends a message while the provider is rate-limited. The gateway
   retries with capped, jittered backoff within the configured budget, streams
   the eventual response, and the run completes with usage and cost metadata.
2. A provider's streaming endpoint is unavailable but its non-streaming
   endpoint works. After the retry budget is exhausted, the gateway performs a
   single non-streaming fallback for that request and emits the response as
   the same event sequence (content, reasoning, tool calls, usage, done).
3. An operator disables streaming fallback for a provider. Requests to that
   provider fail with the underlying streaming error instead of falling back.
4. A user cancels during a retry backoff. The backoff sleep is aborted, the
   request fails closed as cancelled, and the turn stops immediately.
5. A long sequence of failures exceeds the retry budget. The gateway reports
   the error with retry accounting, and the turn fails with a bounded outcome
   instead of stalling.
6. A provider drops a streaming connection mid-response. The gateway surfaces
   the failure as a stream error; consumers own commit boundaries and never
   receive silently restarted content.

## In scope

- Provider registry and per-request provider selection (existing: TOML
  catalogs, OAuth flows, auth resolution).
- Streaming events: content, reasoning, tool-call chunks, usage, done, error
  (existing event contract).
- Chat-completions wire format via the shared genai adapters (existing);
  responses-format compatibility is limited to what genai adapters support.
- Retry/backoff budget: retryable-error classification, capped exponential
  backoff with jitter, per-request retry count, and a total retry-time budget.
- Open-phase status inspection: HTTP status failures the transport defers to
  the stream are classified as open-phase failures and get the same bounded
  retry treatment.
- Per-provider streaming fallback to a non-streaming request after the retry
  budget is exhausted on retryable failures, producing the standard event
  sequence.
- Usage/cost metadata attached to every completed response, including
  streaming responses (usage capture enabled on the request).
- Cancellation that aborts backoff sleep and in-flight recovery.

## Out of scope

- Prewarm and sticky routing (session-scoped transport state, warmup
  requests): tracked as a later F-02 slice; the gateway is stateless per
  request in this PRD.
- WebSocket transport; piko's providers are HTTP-based (SSE) through genai, so
  transport fallback is adapted as stream → non-streaming (see Fusion
  decisions).
- Mid-stream restart of a request that has already delivered content: deltas
  are consumed incrementally and committed by the caller, so silently
  restarting would corrupt transcripts. Mid-stream failures surface as errors
  and recovery is the caller's step-level retry.
- Retry taxonomy for tool execution, persistence, or hostd operations: the
  budget here applies to model requests only.
- Budget planning for prompt tokens (`F-04 context-management`).
- Per-turn usage accounting and telemetry dashboards (`F-15 observability`).

## Behavior and states

### Request lifecycle

`chat_stream` receives a `GatewayRequest` and returns a `Stream<GatewayEvent>`
without performing any side effect at call time. Middleware `pre_chat` hooks
run once per request, before any transport attempt. The request then passes
through:

1. **Open with retry.** The gateway opens a streaming request and inspects the
   first events so HTTP status failures that the transport defers to the
   stream (503, 429, 401, …) are classified as open-phase failures. On a
   retryable failure, it waits a capped, jittered backoff delay and retries
   until the attempt budget or the total retry-time budget is exhausted.
   Retryable failures are classified structurally (HTTP status codes,
   transport errors, wrapped status errors) with string fallback.
2. **Consume.** The stream is consumed and mapped 1:1 to `ContentDelta`,
   `ReasoningDelta`, `ToolCallChunk`, `Usage`, and a final `Done`. A failure
   or premature end mid-stream surfaces as `GatewayEvent::Error` with the
   existing semantics; the gateway never restarts a stream after content has
   been delivered.
3. **Non-streaming fallback.** When the retry budget is exhausted on
   *retryable* failures and the provider has streaming fallback enabled, the
   gateway makes one non-streaming completion with the same request and
   options, and emits the same event sequence derived from the full response.
   Non-retryable failures (auth, bad request) and cancellation fail
   immediately without fallback. If fallback is disabled or fails, the
   request returns `Err` with the underlying error.
4. **Usage.** Every completed response — streaming or fallback — carries a
   `Usage` event before `Done` when the provider reports usage; token and cost
   middleware annotate it.

### Retry classification

A failure is retryable when it is a transient transport or rate-limit
condition:

- HTTP status 408, 409, 425, 429, 500, 502, 503, 504, 520–529, including
  status errors wrapped inside stream errors by the transport.
- Transport errors (connect, timeout) surfaced by the HTTP client.
- Malformed stream data and mid-stream disconnects (surfaced as stream
  errors).
- String fallback for provider-specific transient wording.

Authentication (401/403), invalid requests (400/422), and missing-model errors
are not retryable and fail immediately.

### Backoff and budget

- Backoff delay for retry *n* (1-based) is
  `min(base_delay_ms × 2^(n−1), max_delay_ms)`, multiplied by jitter in
  `[0.9, 1.1]`.
- A retry is only scheduled if the total elapsed retry time plus the next
  delay fits within `budget_ms`.
- The budget and attempt count are per request and shared across every open
  attempt of that request.
- Disabling retries (`enabled: false`) disables backoff and fallback.

### Failure states

- Retry budget exhausted in the open phase: `chat_stream` returns `Err`
  describing the last error; if fallback is enabled and the failure was
  retryable, one non-streaming completion is attempted first.
- Non-retryable open failure (auth, bad request) or cancellation:
  `chat_stream` returns `Err` immediately with no retries and no fallback.
- Mid-stream failure or premature end without `Done`: `GatewayEvent::Error`
  is yielded, followed by stream termination; no `Done` is emitted.
- Cancelled during backoff: the request fails closed without waiting out the
  sleep; a cancelled consume yields the existing `Done("abort")` behavior.

### Non-streaming calls

`llm_call` (raw, stateless completion used by hostd) honors the same retry
policy and budget. Streaming fallback does not apply because the call is
already non-streaming.

## Acceptance criteria

- [ ] Retryable transport/rate-limit failures (503/429/timeout) retry with
      capped, jittered backoff; attempts stop at `max_retries` or when the
      next delay exceeds the remaining budget (differential: codex-rs
      `responses_retry.rs` budget/retry-loop semantics).
- [ ] Non-retryable failures (401/400/422) fail immediately without retries
      and without fallback.
- [ ] When the retry budget is exhausted and fallback is enabled, the gateway
      completes the request non-streaming and emits the standard event
      sequence (content/reasoning/tool calls, usage, done).
- [ ] Streaming fallback is controllable per provider; when disabled, the
      request fails with the streaming error.
- [ ] Cancellation during backoff aborts immediately and never waits out the
      sleep.
- [ ] Every completed response emits one `Usage` event with input, output,
      cache, and total token counts before `Done` when the provider reports
      usage (streaming requests enable usage capture).
- [ ] A stream that breaks mid-response surfaces as an error without a
      silently restarted or duplicated response.
- [ ] `RetryConfig` budget and cap settings flow from `settings.toml`
      (`[retry]`) through hostd to the gateway.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| What bounds retry time? | `budget_ms` total retry-time budget plus `max_delay_ms` per-attempt cap | Prevents unbounded stalls while keeping exponential backoff useful; per-request budget is simple and observable |
| Default retry budget? | 60 s total, 30 s max delay, 3 retries, 2 s base | Matches current defaults and keeps worst-case wait within a user-visible bound |
| What counts toward the budget? | Actual backoff sleep time, shared across every open attempt of the request | One budget per request regardless of how many attempts it takes |
| When does fallback trigger? | Only after the retry budget is exhausted on retryable failures | Auth/bad-request failures and cancellation fail immediately; a fallback that repeats the same auth failure wastes a request |
| Is fallback per-provider? | Yes, `streamingFallback` opt-out on `ProviderConfig`, default enabled | Providers differ in streaming reliability (digest: "per-provider streaming fallback") |
| How are mid-stream breaks handled? | Surfaced as a stream error; no gateway-side restart after content delivery | Callers consume deltas incrementally and commit at step boundaries, so a silent restart would corrupt transcripts; step-level retry is the recovery path |
| Are streaming responses asked to report usage? | Yes (genai `capture_usage`) | Completed turns need token/cost accounting; providers that do not report usage simply omit the event |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Retry loop with count + budget, restarting the sampling request from history | **kept (adapted)** | `piko-llmd` retries the same frozen `GatewayRequest` (prompt is immutable per step) with budget/backoff. Codex restarts mid-stream and rebuilds the prompt from history; piko does not restart mid-stream because callers commit deltas incrementally |
| WebSocket → HTTPS transport fallback after retry budget | **kept (adapted)** | piko has no WebSocket transport; the piko-native fallback is stream → non-streaming completion, surfaced as the same event stream |
| Exponential backoff with jitter | **kept (adapted)** | Adds a configured per-attempt cap and total budget (codex caps implicitly via stream retry count); jitter range matches codex `[0.9, 1.1]` |
| Server-provided retry delay (`Retry-After`) | **rejected for now** | genai does not expose response headers on errors; backoff is config-derived. Revisit if a consumer needs it |
| Prewarm and sticky session transport state | **rejected for this slice** | No WebSocket session to warm in piko; tracked as a later F-02 slice if a consumer appears |

## Open questions

1. Should the budget be per request or per turn (shared across model steps)?
   This PRD uses per request; per-turn budgeting would couple gateway state to
   the run and belongs to observability/budget planning (F-15).

## Reference evidence

- codex-rs `core/src/responses_retry.rs` (+ `responses_retry_tests.rs`) —
  retry/fallback loop semantics.
- codex-rs `core/src/util.rs` — `backoff()` jitter range and factor.
- codex-rs `core/src/client.rs` — transport fallback activation after retry
  budget exhaustion.
- piko `packages/llmd/src/executor.rs` and `packages/llmd/src/stream.rs` —
  streaming, retry/backoff budget, status peeking, and fallback.
