# D-49: Agent run trajectory

> Status: implemented for capture; web viewer retired by
> [ADR-029](../decisions/ADR-029-retire-trajectory-web-viewer.md)
> Implements: [F-36](../features/F-36-agent-run-trajectory.md)
> Historical note: the decision to exclude a TUI surface is superseded by
> F-52/D-69. D-49 remains the diagnostic capture design. The loopback HTTP/SSE
> viewer in section 4 is retired; Session History inspects required facts and
> attaches trajectory as diagnostics.

## Goal

Deliver a durable, content-complete per-run record — prompt assembly (input
side) plus agent trajectory (interaction side) — stored in the session journal
as observational event types, then retire D-30 prompt debugging and the OTel
span export. Inspection is Session History (F-52), not a trajectory HTTP
surface.

## Constraints and non-goals

- Journal schema and event-decoding semantics are unchanged: trajectory
  records use the existing optional/ignorable event mechanism
  (`RawEvent::optional`), so they never alter the acknowledged session-state
  projection.
- Capture is best-effort and side-effect-free: a trajectory write must never
  block, fail, delay, or alter a turn.
- No content truncation, no body-capture switch, no retention eviction.
- Streaming deltas, cross-process capture, and a TUI that treats trajectory
  as its history model are out of scope. F-52 is a separate journal-derived
  inspector.
- No changes to compaction, usage accounting, approvals, or turn-runtime
  semantics.

## Proposed design

### 1. Observational journal events (`piko-protocol`)

Trajectory records are appended as optional journal events with event types
prefixed `trajectory.`. The session-store aggregate ignores them (unknown +
`ignorable`), so session-state replay is unaffected; the trajectory reader
decodes them with its own versioned DTOs.

Protocol DTOs (camelCase, versioned):

```rust
pub struct TrajectoryIdentity {
    pub session_id: String,
    pub agent_instance_id: AgentInstanceId,
    pub run_id: String,
    pub execution_id: String,
    pub source_turn_id: Option<String>,
}

pub struct TrajectoryAssemblyRecord {
    pub identity: TrajectoryIdentity,
    pub assembly_version: u32,
    pub prompt_digest: String,
    pub prompt: SemanticRunPrompt,
    pub tool_catalog: ResolvedToolCatalog,
    pub recorded_at: i64,
}

pub struct TrajectoryModelStepRecord {
    pub identity: TrajectoryIdentity,
    pub step_id: String,
    pub provider: String,
    pub model: String,
    pub request: serde_json::Value,
    pub options: serde_json::Value,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<u64>,
    pub retries: Vec<TrajectoryRetryAttempt>,
    pub fallback: Option<TrajectoryFallback>,
    /// Response for steps that never committed; committed responses are
    /// replayed from the journal message referenced by `message_id`.
    pub response: Option<serde_json::Value>,
    pub message_id: Option<String>,
}

pub struct TrajectoryToolCallRecord {
    pub identity: TrajectoryIdentity,
    pub call_id: String,
    pub tool_name: String,
    pub arguments: Option<serde_json::Value>,
    pub status: TrajectoryToolCallStatus, // started/running/awaiting_approval/completed/failed/cancelled
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub message_id: Option<String>,
}

pub struct TrajectoryChildRunRecord {
    pub identity: TrajectoryIdentity,
    pub child_agent_instance_id: AgentInstanceId,
    pub child_run_id: Option<String>,
    pub spawned_at: i64,
    pub completed_at: Option<i64>,
}

pub struct TrajectorySystemNotificationRecord {
    pub identity: TrajectoryIdentity,
    pub kind: TrajectoryNotificationKind,
    // approval_requested | approval_resolved | steer_delivered | run_error |
    // tool_denied | context_warning
    pub summary: String,
    pub recorded_at: i64,
}

pub enum TrajectoryRecord {
    Assembly(TrajectoryAssemblyRecord),
    ModelStep(TrajectoryModelStepRecord),
    ToolCall(TrajectoryToolCallRecord),
    ChildRun(TrajectoryChildRunRecord),
    SystemNotification(TrajectorySystemNotificationRecord),
}
```

Event type strings: `trajectory.assembly`, `trajectory.model_step`,
`trajectory.tool_call`, `trajectory.child_run`,
`trajectory.system_notification`. One record per journal event. Tool calls
emit one event per status transition (arguments carried on the first). Each
event is `RawEvent::optional` with version `1`.

### 2. Capture path (`piko-orchd-api`, `piko-orchd`, `piko-hostd`, `piko-llmd`)

Add a hostd-implemented port to `SessionExecutionPorts`:

```rust
#[async_trait]
pub trait TrajectoryCapturePort: Send + Sync {
    async fn record(&self, record: piko_protocol::TrajectoryRecord);
}
```

The hostd implementation never fails the caller: it enqueues into a bounded
channel per session store and returns `Ok(())` immediately. A dedicated writer
task drains the channel and appends each record via the existing journal
append path (`Store::append` with `ProposedCommit::one` + `RawEvent::optional`),
serializing revision advances under the session-store lock. When the channel
is full or an append fails, the record is dropped and an in-memory per-run
`dropped_records` counter is incremented; the query surface reports it.

Capture points (all existing production boundaries, no parallel debug path):

| Record | Capture point | Owner |
|---|---|---|
| Assembly | `HostPromptAssemblyPort::assemble_prompt` (D-30's in-memory map is replaced by this record) | hostd |
| Model step | llmd gateway boundary (request/options/timestamps/retry/fallback; the D-30 `record_model_input` hook is redirected here). Response/thinking for committed steps is replayed from the journal message referenced by `message_id`; uncommitted steps carry `response` inline | llmd → hostd sink |
| Tool call | orchd tool-batch executor at call start and each status transition/finish; results for committed calls are replayed from journal messages | orchd |
| Child run | orchd multi-agent spawn and completion | orchd |
| System notification | hostd approval port, steer commit, run-error path | hostd |

Model-step and tool-call records are produced through the same
`TrajectoryCapturePort` instance attached per session, so isolation is by run
identity inside the record itself; concurrent runs never share state.

### 3. Query and replay (`piko-hostd`)

A `TrajectoryQuery` service replays the session journal's raw events (using a
small public raw-event iterator added to `piko-session-store`; no format
change), filters `trajectory.*` events plus relevant facts
(`execution_started`, `execution_finished`, `message_committed`,
`usage_recorded`, `compaction_recorded`), and joins them by run identity:

- **List runs**: scan for `trajectory.assembly` records; return
  `TrajectoryRunSummary { identities, started_at, finished_at, step_count,
  tool_call_count, child_run_count, terminal, dropped_records }`, newest
  first, cursor-paged.
- **Fetch one run**: assembly record + ordered `TrajectoryRecord`s + the run's
  committed messages (paged), plus terminal state from `execution_finished`
  (or "interrupted" when absent).
- Missing runs return an explicit 404-style error; queries never attach a
  session, start a run, or invoke the model gateway.

### 4. Web viewer (`piko-hostd`) — retired (ADR-029)

The loopback HTTP + SSE viewer, static assets, live broadcast contract, and
`[trajectory]` bind/port/enabled settings are removed. Capture, `trajectory.json`,
and Session History diagnostic enrichment remain. The original design below is
historical.

Add an HTTP server dependency (`axum`) to `piko-hostd`; tokio net is already
available. New settings section:

```toml
[trajectory]
enabled = false   # dev: true
bind = "127.0.0.1"
port = 3847
```

When enabled, hostd binds loopback only and prints the viewer URL to stderr.
Bind failure is non-fatal: hostd logs the error and continues without the
viewer.

Endpoints (all read-only):

- `GET /` — static single-page viewer (`include_str!` HTML with inline CSS/JS;
  no frontend toolchain). Two-column layout: session list (left), and on the
  right a run selector above a message-type track timeline (tracks per role,
  brick-laid left-to-right in journal/timestamp order with fixed slots) above
  a chronological message list. Bricks never overlap (no time-scale
  compression); a time ruler shows run bounds and hover tooltips show real
  timestamps. Both timeline bricks and message rows highlight the same
  message. Deliberately framework-free; adopt Svelte only if virtualized
  tracks or playhead/zoom interactions become necessary.
- `GET /api/trajectory/runs?session_id=..&agent_instance_id=..&cursor=..&limit=..`
  — run list, newest first.
- `GET /api/trajectory/runs/{run_id}` — full run record (paged).
- `GET /api/trajectory/runs/{run_id}/stream` — SSE (`text/event-stream`),
  `EventSource`-compatible. Initial snapshot followed by live records pushed
  as they are durably appended; event ids carry the journal record sequence so
  reconnects can resume with `Last-Event-ID`.

Live fan-out: the trajectory writer task publishes each appended record to a
tokio broadcast channel keyed by run; the SSE endpoint subscribes and forwards
records. The page renders the step graph with foldable steps, content, and
child-run links; oversized payloads are lazy-loaded/collapsed at render time.

### 5. Retirements

**D-30 prompt debugging** — delete once the query path and viewer are
verified:

- `piko-protocol`: `PromptDebugSnapshot`, `ModelInputDebugSnapshot`,
  `Command::PromptDebugGet`, `CommandResult::PromptDebugged`, and round-trip
  tests.
- `piko-hostd`: `prompt_debug_snapshots` map, `prompt_debug_snapshot` port
  method, the `PromptDebugGet` dispatch branch, the prompt-input buffer in
  telemetry (`begin_prompt_run`/`model_inputs`), and their tests.
- `piko-tui`: `/prompt-debug` slash entry, `request_prompt_debug`,
  `DiagnosticsKind::PromptDebug`, `set_prompt_debug`, the response branch, and
  related tests/docs.
- Mark D-30 superseded and update F-15's prompt-debugging slice in the feature
  index.

**OTel span export and GenAI content**:

- `piko-hostd/logging.rs`: stop installing the span exporter and `OtelLayer`;
  keep the metrics exporter and the logs bridge. Internal `tracing` spans
  remain for local console correlation but are never exported.
- `piko-llmd`: delete `genai_telemetry.rs`, the `capture-content` path, and
  D-46 verification tests; keep metrics (TTFT/TTFM, retry/fallback counters,
  token/cost) and log correlation attributes (`session_id`, `run_id`,
  `agent_instance_id`, `step_id`).

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Trajectory DTOs and event-type constants; remove prompt-debug DTOs, command, result, and tests |
| `piko-hostd` | Trajectory recorder port impl + writer task, query service, D-30 removal, OTel span-export removal; HTTP/SSE viewer later retired by ADR-029 |
| `piko-orchd` | Tool-call and child-run capture at existing boundaries via the new port |
| `piko-orchd-api` | `TrajectoryCapturePort` trait + `SessionExecutionPorts` field |
| `piko-llmd` | Model-step record capture (redirect existing hook), remove GenAI content attributes |
| `piko-session-store` | Public raw-event iterator only; no schema or format change |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Capture enqueue failure (channel full) or append failure drops the record,
  increments the per-run `dropped_records` counter, and is reported by the
  query; the turn is never affected.
- An interrupted run (hostd restart or abort) keeps every record durably
  written before interruption and queries as interrupted (no terminal state).
- Viewer HTTP/SSE failure modes no longer apply (ADR-029). Capture enqueue or
  append failure still drops the record and never affects the turn.
- Older readers ignore `trajectory.*` events by construction (optional event
  class), so schema evolution and downgrade remain safe.

## Verification

- Unit: DTO serde; optional-event append/decode round-trip; writer drop
  counting; query grouping/ordering; page cursors.
- Integration: capture pipeline produces `trajectory.*` journal events and a
  query returns the run graph (assembly + steps + tool calls + child links +
  terminal); restart preserves and re-queries the same run; simulated capture
  failure does not fail the turn and reports `dropped_records`. The HTTP/SSE
  viewer is retired (ADR-029); Session History covers inspection.
- Retirement checks: no `prompt-debug` identifiers remain in the workspace;
  OTel logs/metrics still export with run/step correlation and no span
  exporter is installed; D-46 GenAI content tests are removed.
- `cargo fmt --all`, clippy with warnings denied, and full workspace tests.

## Alternatives considered

- **Separate trajectory store**: rejected — duplicates journal facts, creates
  a second authority, and the journal's optional-event class already gives the
  same durability without replay impact.
- **TUI trajectory surface**: rejected as the history model — F-52 later adds
  a journal-derived inspector and attaches trajectory only as diagnostics.
- **Polling / SSE live updates**: rejected for product inspection — Session
  History is explicit-refresh only (F-52). The original SSE viewer is retired.
- **OTel as the content store**: rejected — span attributes are transport-
  capped and redacted; trajectory is local, unbounded, and replayable.
- **Retention eviction**: rejected — PRD records everything with no eviction.

## Rollout

1. Protocol DTOs, optional-event append path, `TrajectoryCapturePort`,
   hostd recorder + writer, and the query service (assembly capture only).
2. Model-step (llmd hook redirect), tool-call (orchd), child-run, and system-
   notification capture; terminal state via fact replay.
3. Loopback HTTP server, settings, static page, run list/fetch endpoints, and
   SSE live stream (later retired by ADR-029).
4. D-30 removal and OTel span/GenAI-content removal; docs and verification
   (F-15/D-30 status updates, V-49 evidence).
