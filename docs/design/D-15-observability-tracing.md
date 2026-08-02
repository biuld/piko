# D-15: Observability — end-to-end tracing and metrics

> Status: accepted
> Implements: [F-15](../features/F-15-observability.md)

## Goal

Deliver the first F-15 slice: one end-to-end OTel trace per turn
(turn → agent run → model step → tool batch/call → child agent run, with the
llmd retry/fallback/usage detail inside each model step) plus OTel metrics
(TTFT/TTFM, token/cost, retry/fallback, tool and turn timing), exported from
the hostd process over OTLP HTTP and controlled by `settings.toml
[observability]`.

## Constraints and non-goals

- hostd, orchd, and llmd run in one process today; no traceparent plumbing is
  added to the stdio protocol in this slice.
- `piko-orchd` and `piko-llmd` depend only on `tracing`; the OTel SDK and
  exporters live in `piko-hostd` only.
- OTLP HTTP only; gRPC rejected.
- Sampling is 100% during build-out (the point is observing behavior); no
  sampling-ratio knob in this slice.
- Rollout recorder, turn-diff tracking, prompt debugging, and anonymous
  telemetry are later F-15 slices.

## Proposed design

### 1. Export plumbing (hostd)

`HostSettings` gains an `[observability]` section (kebab-case, `Option` fields,
mirroring `[retry]`/`[sandbox]`):

```toml
[observability]
enabled = false
otel-endpoint = "http://127.0.0.1:4318"
service-name = "piko-hostd"
```

`logging.rs::init` composes, when `enabled`:

- a `tracing-opentelemetry` layer backed by an OTLP HTTP tracer provider
  (resource: `service.name`, `host.arch`, `host.name`);
- a meter provider with an OTLP HTTP periodic reader (default 5 s interval);
- an OTel logger provider with an OTLP HTTP log exporter bridged into
  `tracing` via `opentelemetry-appender-tracing`, so every tracing event is a
  LogRecord (unified logging — the hand-rolled file appender and its CLI/env
  plumbing were removed).

`LogGuard` keeps the provider shutdown handles so spans/metrics/logs flush at
exit. When disabled, a plain stderr console layer is installed and no OTLP
traffic leaves the process. `--log-file`/`--no-log` were removed from the
hostd CLI (log level is the only log-related flag).

### 2. Instrumentation conventions

- Span names are `<layer>.<kind>`: `turn.run`, `agent.run`, `model.step`,
  `tool.batch`, `tool.call`, `llm.request`.
- Attributes are the piko correlation keys already flowing through the code:
  `session_id`, `run_id` (`operation_id` at hostd, `execution_id` inside
  orchd/llmd), `agent_instance_id`, `step_id`, plus domain fields
  (`model`, `provider`, `tool`, ...).
- Errors are classified into a stable set reused from the llmd retry
  taxonomy: `auth`, `bad_request`, `rate_limit`, `timeout`, `transport`,
  `server_error`, `stream_parse`, `cancelled`, `unknown`. Span status is
  recorded as `otel.status_code`/`otel.status_description` (mapped by
  `tracing-opentelemetry`).
- State transitions are span events, not separate spans, so one trace stays
  readable: retries, fallback, usage, approvals, cancellation.

#### Span context across task boundaries

`tracing` spans do not cross `tokio::spawn` automatically. The runtime has
three real boundaries; each gets an explicit mechanism:

| Boundary | Mechanism |
|---|---|
| turn.run → agent.run | `AgentRuntime::run_agent` captures `Span::current()` and carries it through the internal `AgentCommand::Run` (orchd-internal, not a protocol DTO); the actor instruments its run-loop future with it |
| agent.run → execution actor | the execution run future is `.instrument(current_span)` at spawn |
| tool.call → child agent.run | same `run_agent` capture; child runs nest because parallel tool groups use `join_all` (no task boundary) and the child run is awaited under `tool.call` |
| llm.request lifetime | the span is created in `chat_stream` and the returned stream future is `.instrument(span)`, so events recorded while the stream is polled (usage, cost, stream errors) stay on the span |

Every other `tokio::spawn` added in this slice must `.instrument()` explicitly.

### 3. Span inventory

This is the contract the slice implements. "Anchor" names the code the span
wraps; "Events" are recorded on the span during its lifetime.

| Span | Owner | Anchor | Parent | Key attributes | Events | Watch for |
|---|---|---|---|---|---|---|
| `turn.run` | hostd | `OrchAgentRunRunner::run_agent` → `run_agent_subscription` completes | — (root) | `session_id`, `run_id` (operation_id), `agent_instance_id`, `cwd`, `source` (interactive/background) | `turn.started`; `turn.completed` (reason) / `turn.cancelled` / `turn.failed` (error class) | total turn duration vs model+tool time; failures/cancellations at the root; concurrent turns |
| `agent.run` | orchd | `AgentRuntime::run_agent` (captures parent) → actor run-loop exit | `turn.run` (root) or `tool.call` (child) | `session_id`, `run_id` (execution_id), `agent_instance_id`, `parent_agent_instance_id`, `agent_id`, `agent_spec_id`, `detached` | `run.started`; `run.completed` (stop reason) / `run.cancelled` | child depth and fan-out; runs producing no model step (spin); detached runs never completing |
| `model.step` | orchd | one `ExecutionActor::run_loop` iteration: `run_model_step` + `execute_and_commit_tools` | `agent.run` | `run_id`, `step_id`, `agent_instance_id`, `model`, `provider`, `thinking`, `context_window`, `prompt_blocks`, `transcript_messages`, `tools` | `usage` (input/output/cache tokens), `cost_usd`, `stop_reason`, `ttft_ms`, `budget_enforced` (reserve/available) | step duration vs `llm.request`; `stop_reason` (`max_tokens`? `tool_use`?); token trend vs context window |
| `tool.batch` | orchd | `execute_and_commit_tools`, one span per group | `model.step` | `run_id`, `step_id`, `mode` (parallel/sequential), `call_count`, `concurrency_cap`, `tool_names` | `batch.completed` (ok/failed/aborted counts) | parallel groups silently serializing (cap=1); high failure ratio; batch duration |
| `tool.call` | orchd (hostd approval events land here via in-process span context) | `execute_sequential_call` / `execute_parallel_group`, per call around `registry.execute_tool` | `tool.batch` | `run_id`, `step_id`, `tool_call_id`, `tool_call_index`, `tool`, `mode`, `args_json` (truncated 4 KB), `route` | `approval.requested` / `approval.approved` / `approval.denied` / `approval.timeout` (emitted by hostd); `result.ok` (output size); `result.error` (code, retryable, truncated message); `result.aborted` | tool duration; approval wait time; error codes (`not_found`, `sandbox`, timeout); huge outputs; aborts on cancel |
| `llm.request` | llmd | `LlmdExecutor::chat_stream` — span created there, returned `resilient_stream` instrumented until exhausted | `model.step` | `run_id`, `step_id`, `model`, `provider`, `streaming=true`, `thinking`, `retry_base_ms`, `retry_max_ms`, `retry_budget_ms`, `retry_max_attempts`, `fallback_enabled` | `retry` (attempt, `delay_ms`, error class, truncated error), `retry_budget_exhausted` (error class), `fallback` (stream→non-streaming), `usage` (input/output/cache_read/cache_write), `cost_usd`, `ttft`, `stream.error` (error class), `stream.done` (reason) | TTFT; retry storms (many `retry` events with growing delay); `retry_budget_exhausted` + `fallback`; mid-stream error after content; usage/cost per step |

#### What to look for (reading a trace)

| Symptom | Signal in the trace |
|---|---|
| Turn slow overall | `turn.run` duration vs sum of `model.step` + `tool.call`; a gap between children means orchestration/commit overhead |
| Model feels slow | `llm.request` TTFT + duration; `retry` events explain stalls before first byte |
| Retry/fallback thrash | multiple `retry` events per `llm.request`, `retry_budget_exhausted`, `fallback` — provider instability or bad endpoint |
| Turn hangs | `llm.request` open with no `ttft` event; `tool.call` with no `result.*`; approval requested but no decision event |
| Tool failure | `tool.call` `result.error` code; `batch.completed` failure count; approval `denied`/`timeout` |
| Context bloat | `model.step` token trend vs `context_window`; repeated `budget_enforced` |
| Multi-agent runaway | deep `agent.run` chains, high fan-out under one `tool.call`, detached runs without `run.completed` |

### 4. Metrics inventory

Metrics are recorded at the same points as spans, through telemetry ports (see
§5), so libraries never touch the OTel SDK:

| Metric | Type | Recording layer | Attributes |
|---|---|---|---|
| `piko.turn.duration_ms` | Histogram | hostd | result, source |
| `piko.turn.calls` | Counter | hostd | result, source |
| `piko.model.step.duration_ms` | Histogram | orchd | model, provider (retried/fallback covered by the llmd retry/fallback counters) |
| `piko.model.ttft_ms` | Histogram | llmd | model, provider |
| `piko.model.tokens` | Counter | llmd | model, provider, token_type (input/output/cache_read/cache_write) |
| `piko.model.cost_usd` | Counter | llmd | model, provider |
| `piko.model.retries` | Counter | llmd | model, provider, error_class |
| `piko.model.streaming_fallbacks` | Counter | llmd | model, provider |
| `piko.tool.duration_ms` | Histogram | orchd | tool, status, mode |
| `piko.tool.calls` | Counter | orchd | tool, status, mode |

### 5. Telemetry ports

- `piko-llmd` defines `GatewayTelemetry` (trait, no-op default) with
  `record_ttft`, `record_usage(model, provider, usage, cost_usd)`,
  `record_retry(model, provider, error_class)`,
  `record_fallback(model, provider)`. `build_gateway` takes an optional
  `Arc<dyn GatewayTelemetry>` (default no-op) and forwards it to the
  middleware/stream layer.
- `piko-orchd-api` defines `RuntimeTelemetry` (trait, no-op default) with
  `model_step_completed(ModelStepTelemetry { model, provider, duration_ms,
  status, retried, fallback })` and `tool_call_completed(ToolCallTelemetry {
  tool, duration_ms, status, mode })`. It is injected through
  `ExecutionServices` alongside the existing ports.
- hostd implements both with OTel meters and wires them at gateway
  construction and execution services setup. Turn metrics are recorded by
  hostd directly around the agent-run subscription.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | none (no DTO changes; trace context stays crate-internal) |
| `piko-hostd` | `[observability]` settings; OTLP HTTP trace/metric/log exporters + `opentelemetry-appender-tracing` bridge in `logging.rs`; `RuntimeTelemetry` + `GatewayTelemetry` OTel implementations; `turn.run` span and turn metrics; approval events on `tool.call` |
| `piko-orchd` | `agent.run`, `model.step`, `tool.batch`, `tool.call` spans; span capture/carry through `AgentCommand::Run`; step/tool metrics via `RuntimeTelemetry` |
| `piko-orchd-api` | `RuntimeTelemetry` trait + telemetry structs (no-op default) |
| `piko-llmd` | `llm.request` span + retry/fallback/usage/ttft events; `GatewayTelemetry` trait + no-op; span handle on `GatewayContext` |

Dependencies added: `opentelemetry`, `opentelemetry_sdk` (logs feature),
`opentelemetry-otlp` (HTTP + traces + metrics + logs),
`opentelemetry-appender-tracing`, `tracing-opentelemetry` — all in
`piko-hostd` only; `tracing-appender` and the file-logging stack were removed.
`orchd`/`llmd` gain no new dependencies.

## Failure and cancellation

- Exporter failures (collector down, bad endpoint) are non-fatal: the OTel
  SDK logs a warning and drops export; the stderr console fallback still
  shows logs when observability is disabled. Hostd must not fail to start or
  serve turns because telemetry is broken.
- Cancelled turns still produce a `turn.run` span with a `turn.cancelled`
  event; partial child spans stay attached so the sequence up to the cancel
  point is readable.
- `tracing-opentelemetry` never panics on span/attribute misuse; attribute
  truncation is handled at the exporter layer, with arg/output bodies
  truncated before recording (no secrets in spans).
- On exit, provider shutdown flushes buffered spans/metrics (bounded wait, no
  indefinite hang).

## Verification

- Unit: settings parsing/defaults for `[observability]`; span-event helper and
  error-classification tests in llmd.
- Integration: in-process test harness builds a subscriber with a test span
  exporter and an in-memory metric reader, runs a scripted turn through the
  existing orchd fixtures, and asserts:
  - trace tree `turn.run → agent.run → model.step → llm.request` with
    `retry` + `fallback` events, `tool.batch → tool.call`, and a `spawn_agent`
    child `agent.run` nested under `tool.call`;
  - every span carries `session_id`/`run_id` and, where applicable, `step_id`
    /`agent_instance_id`;
  - metric points exist for step duration, TTFT, tokens, cost, retries,
    fallbacks, tool duration, and turn duration.
- Differential: acceptance criteria in F-15 map to the fixtures referenced in
  the PRD; existing `cargo test --workspace` suites must stay green with
  observability disabled.
- Manual: run hostd with `[observability]` enabled against a local Jaeger,
  exercise a turn with a transient provider error and a `spawn_agent`, and
  confirm the trace and metrics in the UI.

## Alternatives considered

- **OTel SDK in orchd/llmd directly**: rejected — breaks the hostd-owned
  exporter boundary; libraries keep only `tracing`.
- **Metrics derived from span events via a hostd bridge**: rejected — fragile
  coupling between span representation and metric values; direct telemetry
  ports at the recording points are simpler.
- **OTel links for child agent runs instead of nesting**: rejected for this
  slice — explicit instrumentation keeps a single readable tree; links remain
  the fallback if detached runs prove unreliable to nest (F-15 open question).
- **OTLP gRPC**: rejected by product decision; HTTP only.
- **Cross-process traceparent now**: rejected — single process; revisit only
  if orchd runs standalone.

## Rollout

Small, independently verifiable slices:

1. hostd `[observability]` settings + OTLP HTTP trace/metric providers +
   logging wiring + JSON log fallback. Verify against local Jaeger.
2. llmd `llm.request` span + retry/fallback/usage/ttft events +
   `GatewayTelemetry` metrics.
3. orchd `agent.run` / `model.step` / `tool.batch` / `tool.call` spans +
   child-agent span propagation + `RuntimeTelemetry` step/tool metrics.
4. hostd `turn.run` root + turn metrics + approval events on `tool.call`.
5. `docs/verification/V-15` evidence; F-15 status → implemented.
