# F-15: Observability and runtime debugging

> Status: implemented
> Priority: P1
> Source evidence: codex-rs `core/src/otel_init.rs`, `rollout.rs`,
> `turn_timing.rs`, `turn_metadata.rs` (digest block L); no direct parity
> required (ADR-002)

Slices:
- Tracing + metrics (D-15 / V-15)
- Per-turn usage accounting (D-29 / V-29)
- Prompt assembly debugging (D-30 / V-30) — superseded by F-36/D-49 and
  removed (V-49)
- OTel GenAI prompt inspection (D-46 / V-46)
- Durable rollout paging (D-31 / V-31)
- Exact turn-diff tracking (D-32 / V-32)

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

The debugging slices add read-only inspection of the latest real prompt
assembly and actual llmd model inputs, bounded paging over the existing
durable AgentInstance rollouts, and exact net workspace diffs per turn.

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

- Provider-specific HTTP wire payloads after adapter-internal rendering.
- Anonymous product telemetry / installation ID — no external data leaves the
  machine in this slice.
- Cross-process trace-context propagation: hostd, orchd, and llmd run in one
  process today; this slice does not add traceparent plumbing to the stdio
  protocol. If orchd ever runs standalone, revisit.
- Compaction and auto-budget *policy* that *consumes* turn usage (F-05 remains
  the policy owner; this slice only provides the durable ledger baseline).

## Behavior and states

### Tracing slice (D-15)

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

### Per-turn usage accounting (D-29)

- **Atomic grain:** each completed model step's provider `Usage` is written
  onto the durable assistant message for that step (llmd → orchd).
- **Hostd is the product ledger:** on each new committed assistant message
  with usage, hostd accumulates into (1) the in-flight turn's usage total
  keyed by `source_turn_id` and (2) the session's `cumulative_usage`.
- **Turn total = roll-up of steps** on that turn; session total = roll-up of
  all recorded step usages. OTel metrics are a **projection of the same
  values**, not a second source of truth.
- Terminal turn lifecycle events (`TurnEvent::{Completed,Failed,Cancelled}`)
  carry the turn's rolled-up `usage` so clients observe the ledger without
  re-summing transcript.
- On session resume, hostd rebuilds `cumulative_usage` by walking assistant
  messages in the recovered transcript (messages remain the durable facts).
- Partial turns (failed/cancelled mid-run) still report usage for steps that
  committed before the terminal outcome.

### Prompt assembly debugging (D-30)

- After production prompt assembly succeeds, hostd retains the latest
  in-memory debug snapshot per session and agent instance.
- A snapshot contains the exact `SemanticRunPrompt`, resolved tool catalog,
  and retained prompt-resource messages (`world_state` followed by user mentions)
  used to start that run.
- Reading a snapshot is observational: it does not create a turn, call a
  provider, mutate prompt baselines, or write session storage.
- Before an agent has assembled a prompt, and after hostd restarts before a
  new assembly, the query fails explicitly with "snapshot unavailable".
- New successful assembly atomically replaces the prior snapshot for that
  session/agent. Failed assembly leaves the previous successful snapshot.
- Every snapshot is bound to the exact run that consumed it. Model inputs from
  an older run that arrive after replacement must never be joined to the newer
  snapshot.
- Prompt bodies may contain workspace-controlled or user-mentioned content;
  the snapshot is returned only over the existing local hostd client
  channel and is never emitted to logs or OTel exporters.
- Each model step appends the actual provider-neutral request and options
  produced by llmd after prompt mapping, tool conversion, middleware,
  thinking, and cache policy, immediately before adapter dispatch.
- Model inputs are bounded to the latest 32 steps for the captured assembly.

### OTel prompt inspection (D-46)

- OTel is the transport and correlation substrate for a LangSmith-like trace
  view; it is not a product-state store and hostd does not query it back.
- The trace topology and prompt-assembly provenance are safe metadata and may
  be exported whenever observability is enabled.
- Prompt, transcript, and tool-definition bodies are sensitive content. Model
  input export is separately opt-in, defaults off, and uses the applicable
  OTel GenAI semantic-convention fields at the model-call boundary. Thinking
  blocks and model-output bodies are not exported in this slice.
- Content attributes are size-bounded and must not be duplicated into stderr
  or OTel log records.
- The trajectory viewer (F-36) is the immediate diagnostic surface for prompt
  assembly; its fidelity does not depend on OTel sampling or exporter
  availability. The D-30 latest-only snapshot is removed (V-49).

### Durable rollout paging (D-31)

- `RolloutPageGet` reads one AgentInstance shard with an opaque forward cursor.
- Pages default to 50 items and are capped at 200; `next_cursor` is present
  only when another page exists.
- Reads never create a session store and preserve transcript sequence order.

### Exact turn diffs (D-32)

- Successful built-in `edit` and `write` results durably retain exact before
  and after text in hidden ToolResult details excluded from model-visible output.
- Hostd merges repeated writes as first-before/latest-after, removes net-zero
  changes, and emits a `TurnDiff` event after durable commit.
- `TurnDiffGet` returns live state or reconstructs the same net diff from
  durable rollout shards after restart without reading current files.

## Acceptance criteria

### Tracing + metrics (V-15)

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

### Per-turn usage accounting (V-29)

- [x] Committing an assistant message with usage increases both the turn's
      in-memory usage and the session `cumulative_usage` by that amount.
- [x] Multi-step turns roll up every step's usage into the turn total.
- [x] `TurnEvent::Completed` (and Failed/Cancelled) include the turn usage
      roll-up matching the committed assistant messages for that turn.
- [x] Session resume rebuilds `cumulative_usage` from durable assistant
      messages.
- [x] Hostd records turn-level OTel token/cost counters from the same turn
      ledger at terminal lifecycle (does not invent a second total).

### Prompt assembly debugging (D-30 / V-30) — superseded

This slice is superseded by F-36/D-49: the latest-only in-memory snapshot was
retired once the durable trajectory query path landed (verified in V-49).
Prompt assembly is now served from `trajectory.assembly` journal events by the
trajectory viewer.

### OTel prompt inspection (D-46)

- [x] One trace shows turn → agent → prompt assembly → model call → tool calls,
      with run/step/agent correlation and assembly block provenance.
- [x] Sensitive GenAI content is absent by default and exported only under a
      separate explicit setting.
- [x] The exported opted-in GenAI attributes contain the actual provider-neutral
      model input without piko depending on a backend for correctness.

### Durable rollout paging (V-31)

- [x] Durable transcript items page in sequence order with no overlap.
- [x] Cursors are opaque, invalid cursors fail explicitly, and limits are bounded.
- [x] Reading an unknown session does not create storage.

### Exact turn diffs (V-32)

- [x] Repeated mutations roll up to exact first-before/latest-after net state.
- [x] Net-zero and failed changes contribute nothing.
- [x] Change metadata is durable but absent from model-visible tool output.
- [x] Historical diffs rebuild from rollouts without workspace reads.

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
| Who owns the product usage ledger? | **hostd** turn + session roll-ups; transcript assistant messages are durable step facts | Matches hostd authority for user-visible state; OTel is a projection |
| Natural grain for usage? | Model-step (assistant message); turn = sum of steps; session = sum of all steps | Provider usage arrives per completion; multi-step turns must not discard earlier steps |
| Client surface for turn totals? | `TurnEvent` terminals carry `usage` | Live clients get totals without replaying the whole tree |
| What is the first prompt-debug surface? | Superseded by F-36: the trajectory viewer serves durable run records (assembly + model steps) from the session journal | Faithful by construction; hostd stays authoritative for user-visible diagnostics |
| Persist debug snapshots? | Superseded: assembly is durable as `trajectory.assembly` journal events (F-36) | The journal is the sole durable authority; the D-30 in-memory snapshot is removed |
| Model-input debug boundary | llmd request after mapping/middleware/options, before provider adapter | Faithful to dispatched model input without claiming adapter-private HTTP wire parity |
| Rollout source | Existing per-AgentInstance append-only JSONL | Avoids a second recorder and preserves hostd's durable authority |
| Turn-diff source | Exact successful built-in mutation results | Avoids racy filesystem rereads and permits restart reconstruction |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| `otel_init.rs` per-crate init | **kept (adapted)** | piko puts one `tracing-opentelemetry` layer in hostd's existing `logging.rs` subscriber; libraries only instrument |
| Turn timing metadata (`turn_timing.rs`) | **kept (adapted)** | TTFT/TTFM become OTel histograms recorded at the same points as spans |
| Rollout recorder / diff tracking | **kept (adapted)** | Page existing v3 AgentInstance JSONL; built-in mutation results carry exact durable content and hostd owns roll-up |
| `installation_id.rs` anonymous telemetry | **rejected for this slice** | piko has no product-telemetry decision yet; tracing stays local |
| Usage accounting / cost metadata | **kept (adapted)** | hostd session+turn ledger from durable assistant step usages; OTel turn counters write-through from the same ledger (not a second store) |
| `prompt_debug.rs` standalone next-input builder | **rejected in final state** | The D-30 adaptation was retired once F-36 trajectory assembly records landed (V-49); codex-rs remains only a modeling reference (ADR-002) |

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
