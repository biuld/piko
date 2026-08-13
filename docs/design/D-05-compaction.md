# D-05: Compaction — budget windows, inline compact, and model-visible context tools

> Status: implemented
> PRD: [F-05](../features/F-05-compaction.md)

## Goal

Turn compaction from a single threshold into a budget-window process that
hostd owns end-to-end:

1. A deterministic, auditable auto-compact trigger with hysteresis (one
   rewrite per window, minimum-growth guard, pending guard).
2. A token-budget inline compact (`new_context_window`) that drops history
   without a model call.
3. Model-visible `get_context_remaining` / `new_context_window` tools whose
   numbers come from the same F-04 budget basis the model request uses.
4. A configurable summarizer model with one fallback to the default model
   (piko's adaptation of codex-rs "remote compaction").
5. `[transcript] max-tool-output-tokens` wired into the orchd model view.

## Constraints and non-goals

- hostd stays authoritative for the durable transcript and the compaction
  decision; orchd never rewrites session history.
- The F-04 estimator (`text ≈ ceil(bytes / 3)`, serialized JSON, +16 framing)
  remains the single accounting basis for both the trigger and the budget
  tools, so decisions and dispatches cannot diverge. Hostd occupancy lives in
  the host `TranscriptEstimator` port, whose adapter calls
  `piko_orchd::transcript` (F-32 / D-44).
- Non-root AgentInstance shards are not compacted through root state
  (unchanged); `SessionCompact` still targets the root shard.
- No compaction hooks, no compaction-specific events beyond
  `SessionReconciled`, no provider-side/cloud compaction.
- Protocol changes must be wire-compatible: new fields carry serde defaults
  and old clients keep working unchanged.

## Proposed design

### 1. Domain: `hostd/domain/compaction/mod.rs` — trigger decision + state

`CompactionState` (currently `{ pending }`, unused) becomes the per-session
window state carried on `SessionState`:

```rust
pub struct CompactionState {
    pub pending: bool,
    pub window_number: u64,
    /// Estimated tokens retained by the last compaction (rearm baseline).
    pub rearm_tokens: Option<u64>,
}
```

`CompactionSettings` gains `min_growth_tokens` (default `16_384`). The naive
`should_compact` is replaced by a decision:

```rust
pub enum CompactTrigger {
    Trigger,
    Hold { reason: CompactHoldReason },
    Disabled,
}

pub fn compact_trigger(
    estimate: &ContextUsageEstimate,
    context_window: u64,
    settings: &CompactionSettings,
    state: &CompactionState,
) -> CompactTrigger
```

Rules (exactly as PRD F-05):

```text
disabled                      → Disabled
estimate.tokens + reserve <= window → Hold(UnderHighWaterline)
state.rearm_tokens is None    → Trigger
estimate.tokens − rearm >= min_growth → Trigger
else                          → Hold(InsufficientGrowth)
```

The rearm baseline is written from the retained tail's estimate when a
compaction lands, and `window_number` advances. Because the window state is
derived state (rearm + window number), resume/replay only needs the last
`CompactionEntry.details` — no separate durable file (PRD error-state
requirement).

### 2. Protocol: `SessionCompact` gains a mode

```rust
SessionCompact {
    command_id: CommandId,
    session_id: SessionId,
    agent_instance_id: crate::AgentInstanceId,
    #[serde(default)]
    mode: CompactMode,
}

#[derive(Default)]
pub enum CompactMode {
    #[default]
    Summarize,
    NewContextWindow,
}
```

`Summarize` = today's behavior (manual summarize-and-keep). Old clients that
omit `mode` keep working. The TUI's existing `session.compact` command
remains a `Summarize` invocation.

### 3. Application: `hostd/application/compaction.rs` — reworked flow

`compact_session_if_needed` gains a `mode: CompactMode` parameter (default
`Summarize`) and a `force: bool` that bypasses the trigger decision:

```text
root shard check → build branch entries (unchanged)
manual (force) or compact_trigger(...) == Trigger  → proceed
else → return
pending guard: if state.compaction.pending → return
state.compaction.pending = true
if mode == Summarize:
    find cut point (keep_recent waterline, unchanged)
    resolve summarizer model (settings summarizer-model/provider)
    summarize_history(...)
    on failure with non-default summarizer → retry once with default model
    on failure → clear pending, return (no rewrite)
else (NewContextWindow):
    keep the most recent user message, drop everything before it
    summary = fixed "A new context window was started without summarizing
              conversation history." (no model call)
append CompactionEntry (existing path) with
    details = { "trigger": "auto|manual|new_context_window",
                "windowNumber": n, "tokensBefore": e, "tokensAfter": r }
state.compaction = { pending: false, window_number: n+1, rearm_tokens: r }
emit SessionReconciled (existing path)
```

The `NewContextWindow` cut point is the index of the last
`Message::User` in the branch; with no user message the command fails closed
with a clear error. Cut-point selection is intentionally not shared with
`Summarize` (different retention policy), but the append/reconcile path is
the same code.

### 4. Orchd: `ContextToolsProvider`

New provider in `orchd/src/adapters/tools/context_tools_provider.rs`,
registered by `AgentExecutionRuntime::register_single_agent_tools` (so it is
always present for single-agent runs) with a `context` tool set:

```rust
pub struct ContextToolsProvider {
    callbacks: Arc<RwLock<ContextToolsCallbacks>>,
}

#[derive(Default, Clone)]
pub struct ContextToolsCallbacks {
    pub new_context_window:
        Option<Arc<dyn Fn(String, String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send + Sync>>,
}
```

Tools:

- `get_context_remaining` — read-only; returns
  `{"tokens_left": Option<u64>}` from `ToolExecutionContext.context_remaining`
  (new field, see §5). No host round-trip.
- `new_context_window` — returns the fixed codex message; when no callback is
  wired it fails with a non-retryable `unavailable` error; otherwise it calls
  the host callback with `(session_id, agent_instance_id)`.

Both are `Sequential`, `Never`-approval, no capabilities — metadata tools.

### 5. Orchd: budget threading into tool context

`piko_orchd_api::tools::ToolExecutionContext` gains
`pub context_remaining: Option<u64>`. The actor computes the F-04 budget
estimate before dispatch (existing `enforce_context_budget` call) and passes
`estimate.context_remaining` into `execute_sequential_call` /
`execute_parallel_group`, which store it on the per-call context in
`tool_batch/mod.rs`. This keeps `get_context_remaining` on the exact budget
basis the model request used, with no new service plumbing.

### 6. Settings and config wiring

`HostSettings` gains:

```toml
[compaction]
enabled = true
reserve-tokens = 16384
keep-recent-tokens = 20000
min-growth-tokens = 16384
summarizer-model = "..."     # optional
summarizer-provider = "..."  # optional

[transcript]
max-tool-output-tokens = 24000
```

- `[compaction]` additions are read directly by `compact_session_if_needed`
  from `self.settings` (the summarizer override and hysteresis need no orchd
  involvement).
- `[transcript] max-tool-output-tokens` flows: `HostSettings` →
  `build_orch_turn_runner` → `OrchConfig.transcript_max_tool_output_tokens`
  (new `OrchdConfig` field, default 24_000) → `ModelConfig` in orchd
  (`services.set_model_config`) → the actor builds
  `TranscriptPolicy { max_tool_output_tokens }` instead of
  `TranscriptPolicy::default()`.

### 7. Hostd runner wiring for `new_context_window`

- `OrchAgentRunRunner` holds `context_tools: Arc<ContextToolsProvider>`,
  registers it in `new_with_mcp`, and exposes
  `set_context_window_callback`.
- `AgentRunRunner` (hostd port) gains a default no-op
  `set_context_window_callback(&self, _cb)`; `OrchAgentRunRunner` forwards to
  the provider; `ErrorAgentRunRunner` keeps the no-op.
- In `run_stdio_server` and `ModelRunnerObserver::on_change`, after the
  runner is installed, hostd wires the callback to a closure that calls
  `compact_session_if_needed(command_id, session_id, agent_instance_id, 0,
  tx, CompactMode::NewContextWindow)` on the root shard. The closure captures
  the `HostApp` (`server.0`) and the current client sender.

The callback runs the same host-owned rewrite path as `session.compact`, so
there is exactly one compaction code path regardless of trigger.

## Test plan

Hostd unit (`domain/compaction`):
- trigger decision matrix: disabled, under waterline, first window, rearm +
  growth, rearm + insufficient growth;
- rearm/window advance on compaction landing.

Hostd integration (`tests/compaction_reconcile_cases/`):
- two racing compacts produce one rewrite (pending guard);
- `NewContextWindow` mode drops the prefix, retains the last user message,
  emits `SessionReconciled`, and never calls the summarizer (a gateway that
  panics on `llm_call` proves the no-model-call invariant);
- configured summarizer model is used and failure falls back to the default
  model once (scripted `SummaryGateway` recording call order).

Orchd unit/integration:
- `get_context_remaining` returns the threaded `context_remaining` value and
  `null` when absent;
- end-to-end agent run: model calls `get_context_remaining`, sees the
  expected estimate; `new_context_window` with a wired callback reports the
  fixed message and invokes the callback once (scripted provider);
- `[transcript]` cap: an oversized tool result is truncated at the configured
  cap instead of the default (agent-runtime test with `FauxProvider`).

## Risks

- **Callback lifetime**: the host closure borrows `HostApp`; use `Arc` clones
  (HostApp is already `Clone`) so the callback outlives any single turn.
- **Concurrent turns**: the pending guard runs inside the session state lock
  segment; keep the lock scope minimal so summarization (a network call) does
  not hold the host state lock.
- **Serde compat**: `CompactMode` must have a default so old clients sending
  `session.compact` without `mode` still parse; covered by a protocol
  round-trip test.
