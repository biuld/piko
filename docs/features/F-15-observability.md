# F-15: Observability (end-to-end tracing)

> Status: implemented
> Priority: P1
> Source evidence: codex-rs `core/src/otel_init.rs`, `rollout.rs`,
> `turn_timing.rs`, `turn_metadata.rs` (digest block L); no direct parity
> required (ADR-002)

## Summary

While piko is still being built, a developer must be able to follow one user
turn through the whole runtime as a single trace: hostd turn orchestration →
orchd agent run → model steps → tool batches and individual tool calls →
child agent runs, plus the llmd retry/backoff/fallback and token-usage detail
inside each model step. This PRD covers the first F-15 slice: end-to-end
distributed-trace spans plus OTel metrics (TTFT/TTFM, token/cost,
retry/fallback, tool and turn timing) and unified OTel log records, all
exported out of the hostd process over OTLP HTTP. Logging is unified on OTel:
the hand-rolled file appender is removed, with a stderr console layer as the
only fallback when observability is disabled.

## Problem

Today the runtime emits structured log lines (`tracing` info/warn/error) keyed
by `run_id` and, in places, `step_id`. That is enough to grep for a single
event, but not to answer "what did this turn actually do and where did time
go": retries and fallback transitions in llmd, tool batch parallelism,
multi-agent supervision, and the causal nesting of model steps inside a turn
are invisible unless reconstructed by hand. As the agent loop grows, missing
causality makes regressions slow to localize.

## User journeys

1. A developer runs hostd with tracing export enabled, executes a turn that
   hits a transient provider error, and opens Jaeger.
2. The turn appears as one trace; the failed model step shows each retry
   attempt with delay and the fallback to a non-streaming completion.
3. The developer selects a tool call span, sees its parent model step, its
   duration, and any child agent run it spawned, and spots the slow path
   without reading a log file.

## In scope

- A single root span per turn that nests the agent runtime's causal tree:
  agent run → execution/model step → tool batch → individual tool call →
  spawned child agent run (linked to the parent trace).
- Model-gateway spans inside each model step: request open, retry waits,
  budget exhaustion, streaming fallback, and usage/cost recorded as span
  attributes/events.
- OTel metrics recorded at the same points: model-step duration and TTFT
  histograms, token/cost counters, retry/fallback counters, tool-call and
  turn duration/result metrics.
- Every `tracing` event exports as an OTel LogRecord (unified logging via the
  OTel logs signal); events inside spans carry trace/span context.
- One tracing subscriber in the hostd binary; `piko-orchd` and `piko-llmd`
  emit spans only and never initialize exporters.
- OTLP export controlled from `settings.toml` (`[observability]`, mirroring
  the existing `[retry]`/`[sandbox]` sections; disabled by default). When
  disabled, a stderr console layer keeps local logs visible; there is no file
  logger.
- OTLP HTTP transport only (no gRPC in this slice).
- Correlation attributes on every span: `session_id`, `run_id`, and (where
  applicable) `agent_instance_id`, `step_id`, `model`, `provider`,
  `tool_call_id`.

## Out of scope

- Rollout recorder, turn-diff tracking, and prompt debugging (later F-15
  slices; digest block L).
- Anonymous product telemetry / installation ID — no external data leaves the
  machine in this slice.
- Cross-process trace-context propagation: hostd, orchd, and llmd run in one
  process today; this slice does not add traceparent plumbing to the stdio
  protocol. If orchd ever runs standalone, revisit.

## Behavior and states

- A completed turn produces one root span whose children reflect actual
  causality: model steps are children of their agent run, tool calls are
  children of the step that issued them, spawned agent runs appear as linked
  child traces rooted in the same session.
- A model step that retries records one span event per attempt with
  `attempt`, `delay_ms`, and error class; budget exhaustion and the
  stream→non-streaming fallback are distinct events on the same step span.
- Usage and cost for a completed model step are recorded on that step's span
  (`input_tokens`, `output_tokens`, `cache_read_tokens`, `cost_usd`).
- When export is disabled, the same spans/events are still emitted to the
  stderr console layer and no OTLP traffic leaves the process.
- Error/cancellation: a cancelled or failed turn still yields a root span
  whose error event carries the failure reason; partial child spans remain
  attached.

## Acceptance criteria

- [x] With OTLP export enabled, a turn with one tool call and one transient
      provider failure produces a single trace: turn root → agent run →
      model step (with retry + fallback events) → tool call, verifiable in a
      collector (V-15).
- [x] With OTLP export enabled, the same turn produces metric points for
      model-step duration, TTFT, token/cost counters, retry and fallback
      counters, and tool-call duration/result (V-15).
- [x] A `spawn_agent` tool call produces a child agent trace linked to the
      parent turn trace, correlated by `agent_instance_id`.
- [x] Every span carries `session_id` and `run_id`; model steps additionally
      carry `step_id`, `model`, and `provider`.
- [x] With export disabled, the same events are visible on the stderr console
      and contain the full attribute set.
- [x] `cargo test --workspace` passes with no span-related behavioral
      changes to existing event streams.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Who owns the exporter? | hostd binary, one subscriber | orchd/llmd are in-process libraries; a single subscriber covers the whole trace and keeps libraries exporter-free |
| Where does the toggle live? | `settings.toml` `[observability]` (e.g. `enabled`, `otel-endpoint`), default off | Hostd owns settings; matches the existing `[retry]`/`[sandbox]` section pattern |
| Logging model | Unified on OTel logs; no file appender; stderr console only as disabled-mode fallback | One managed pipeline for logs; no self-built logging stack |
| Default OTLP endpoint | Local collector (`http://127.0.0.1:4318`), configurable | Standard OTLP HTTP port works with Jaeger/OTel collector without extra flags |
| OTLP transport | HTTP only; gRPC rejected | Jaeger and OTel collector both accept OTLP HTTP; one transport keeps the slice small |
| Trace correlation across layers | Spans with structured attributes (`run_id`, `step_id`, `agent_instance_id`) | Single-process runtime needs no traceparent plumbing today |
| Retry/fallback representation | Span events on the model-step span, not separate spans | Keeps one step readable; attempt/delay/cause stay in one place |
| Metrics in this slice? | Yes — step/TTFT histograms, token/cost counters, retry/fallback counters, tool/turn timing | Aggregate trend visibility during build-out; recorded at the same points as spans via a no-op-default telemetry port |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| `otel_init.rs` per-crate init | **kept (adapted)** | piko puts one `tracing-opentelemetry` layer in hostd's existing `logging.rs` subscriber; libraries only instrument |
| Turn timing metadata (`turn_timing.rs`) | **kept (adapted)** | TTFT/TTFM become OTel histograms recorded at the same points as spans |
| Rollout recorder / diff tracking | **deferred** | Separate later slices; digest block L lists them as gaps |
| `installation_id.rs` anonymous telemetry | **rejected for this slice** | piko has no product-telemetry decision yet; tracing stays local |

## Open questions

1. For detached child agent runs, should the child `agent.run` span nest under
   the spawning `tool.call` (via explicit instrumentation) or use OTel links?
   Default in D-15: nest via instrumentation; revisit if nesting proves
   unreliable across task boundaries.

## Reference evidence

- codex-rs `core/src/otel_init.rs` — exporter init pattern.
- codex-rs `core/src/turn_timing.rs`, `turn_metadata.rs`, `responses_metadata.rs`
  — what a model step trace should expose.
- piko `packages/hostd/src/logging.rs` — existing subscriber composition.
- piko `packages/llmd/src/stream.rs` — retry/fallback events to surface.
