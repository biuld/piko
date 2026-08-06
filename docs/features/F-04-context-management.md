# F-04: Context management — transcript accounting, snapshots, truncation, and world-state diffing

> Status: implemented (slice 1); implemented (slice 2, world-state diffing)
> Priority: P0
> Source evidence: codex-rs `core/src/context_manager/{history,normalize,updates}.rs`,
> `core/src/context_manager/token_budget.rs`, `core/src/tools/context.rs`
> (tool-result truncation); `core/src/context/world_state/*` (slice 2)

## Summary

The agent runtime keeps a per-execution transcript that the model sees on
every step. That transcript is copy-on-write and shareable: it carries a
per-message token accounting based on one documented estimator, and it can
produce a normalized *model view* in which oversized tool output is
truncated with an explicit, model-visible marker. Every model request is
admitted only after a fail-closed budget preflight that accounts the exact
normalized view being dispatched, so the model never receives a transcript
whose size differs from what was checked, and a single oversized tool result
can no longer push the request past the context window without anyone
knowing.

## Problem

Today the runtime transcript is a plain in-memory `Vec<Message>` that is
fully cloned on every model step, token estimates are recomputed ad hoc at
preflight time, and oversized tool results are sent to the model verbatim.
That has three consequences:

1. **No accounting to trust**: the budget preflight recomputes estimates by
   re-serializing every message each step, and nothing records per-message
   tokens. Operators cannot see which messages consumed the window, and any
   future budget tool or compaction trigger has no stable per-message basis.
2. **No cheap sharing**: cloning the whole transcript for telemetry, the
   gateway request, and the preflight duplicates work and memory; checkpoints
   and rollback give no way to share one immutable view across a step.
3. **No truncation**: a tool that emits megabytes of output sends all of it
   to the model. The only current defense is the fail-closed preflight, which
   rejects the turn with "compaction required" — an entire turn can fail
   because one tool result was too large, even though most of that output is
   already available in session history or on disk.

## User journeys

1. An agent runs a tool that prints 2 MB of output. The next model request
   contains a truncated tool result with an explicit marker stating how many
   characters were retained, the turn continues normally, and the full output
   is still visible in the committed session transcript.
2. A session grows past the model's context window. The turn fails closed
   with a "compaction required" error that now reports `context_remaining`
   and the per-message accounting basis, so an operator can see exactly how
   the window was consumed and decide to compact (F-05).
3. A developer inspects a long session. Telemetry for each model step records
   how many tool outputs were truncated and how much context remained, so
   budget pressure is observable instead of invisible.

## In scope

- Per-message token accounting on the orchd runtime transcript, maintained
  incrementally on every push/rollback, using one documented conservative
  estimator shared by accounting and the budget preflight.
- Copy-on-write transcript snapshots: an immutable view of messages plus
  their token estimates, shared cheaply (single `Arc`) across model steps,
  telemetry, and preflight; invalidated by any mutation; existing
  checkpoints/rollback preserved.
- Transcript normalization for model requests: a deterministic projection in
  which tool-result text above a configurable cap is truncated to the head
  with an explicit model-visible marker (retained/total characters), while
  image blocks, error flags, and metadata are preserved and the committed
  transcript retains the full output.
- Budget preflight consumes the exact normalized snapshot being dispatched;
  over-budget requests fail closed with `ContextBudgetExceeded` reporting
  `context_remaining`; step telemetry records truncated-output count and
  context remaining.

## Out of scope

- F-05 compaction: auto-compact triggering, budget windows, remote
  compaction, and dropping old transcript messages to fit the window (today
  that stays a hostd-owned decision; orchd fails closed with "compaction
  required").
- Model-visible `get_context_remaining` / `new_context_window` tools
  (follow-on F-04 slice; this slice establishes the accounting they need).
- Settings/protocol wiring for the truncation cap (this slice ships a
  documented constant default; a `[transcript]`-style setting is a follow-up
  owned with F-05 budget windows).
- World-state diffing across turns (deferred from F-03 to a later F-04
  slice).
- Any change to hostd compaction decisions or session-tree persistence; the
  committed transcript stays byte-for-byte the same as today.

## Behavior and states

### Transcript accounting

- Every push (`user`, `assistant`, `toolCall`, `toolResult`, `context`,
  steering) appends the message and its token estimate; the manager's total
  is the sum of per-message estimates.
- `rollback(checkpoint)` restores both messages and estimates, so totals
  never drift after a rollback.
- Snapshots and estimates are derived from the same estimator:
  `text ≈ ceil(bytes / 3)`, JSON at serialized byte cost, images at encoded
  bytes + framing, +16 framing tokens per message. The estimator is
  deliberately conservative; real usage (F-15) may differ and is recorded
  separately.

### Snapshots (copy-on-write)

```text
push(...) → generation += 1, cached snapshot dropped
snapshot() → cached Arc<TranscriptSnapshot> if generation unchanged
model_view(policy) → normalized messages + estimates + truncated count
rollback(checkpoint) → messages/tokens truncated, generation += 1
```

- `snapshot()` returns the same allocation on repeat calls until the next
  mutation (shareable across model steps, telemetry, and preflight).
- `model_view(policy)` is a pure projection of the committed messages; it
  never mutates the manager and never touches the committed transcript.

### Normalization and truncation

- A `ToolResult` whose text blocks exceed `max_tool_output_tokens`
  (default 24,000 estimated tokens ≈ 72 KB) is projected with:
  - the head of the text preserved up to the cap, cut on a character
    boundary;
  - an explicit marker appended:
    `[Tool output truncated: retained N of M characters. The full output is
    preserved in session history — read the file or re-run the tool to
    inspect the remainder.]`;
  - non-text blocks (images) preserved;
  - `details`, `is_error`, `tool_name`, and `tool_call_id` unchanged.
- Below the cap, messages pass through unchanged (same bytes).
- The committed transcript (what hostd persists) always keeps the full
  output; truncation exists only in the model view.

### Budget preflight

- Fixed overhead (prompt, tool schemas, output reserve, reasoning reserve,
  safety margin) is unchanged and still fails closed first.
- Transcript cost is `snapshot.total_tokens()` of the normalized model view
  — the exact messages dispatched to the gateway.
- Over budget → `ContextBudgetExceeded` with
  `estimated request`, `fixed`, `transcript`, `context_remaining`, `window`
  and the existing "compaction required" guidance.
- Success returns the budget estimate so the actor can record
  `context_remaining` and `truncated_outputs` in step telemetry.

## Acceptance criteria

- [x] Per-message token estimates are tracked by the transcript manager; the
      running total equals the sum of estimates and stays consistent across
      push and rollback (unit evidence).
- [x] Two consecutive `snapshot()` calls share one allocation
      (`Arc::ptr_eq`); any mutation invalidates the cache and produces a new
      snapshot (unit evidence).
- [x] A tool result above the cap is truncated in the model view with an
      explicit marker containing retained/total counts; small results,
      image blocks, and error metadata pass through; the committed
      transcript retains the full output (unit + end-to-end evidence).
- [x] The budget preflight accounts the normalized model view and reports
      `context_remaining` on rejection; the estimate basis is identical to
      what is dispatched to the gateway (unit evidence).
- [x] End-to-end: a tool emitting output above the cap yields a truncated
      marker in the next model request while the run completes normally and
      the committed transcript holds the full output (integration evidence).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where does truncation live? | orchd model-view projection only | hostd stays authoritative for durable transcript content; the model view is an execution concern (ADR-002, hostd-authoritative invariant) |
| What happens when the window is still exceeded after truncation? | Fail closed with `ContextBudgetExceeded` ("compaction required") | F-05 owns dropping old messages; silently trimming history in orchd would split authority for user-visible state |
| What does the marker tell the model? | Kept/total characters + how to recover the rest | The model must know output was elided and how to get the remainder (read file / re-run tool) |
| One estimator or many? | One documented conservative estimator shared by accounting and preflight | Estimates must match what is dispatched; a single basis makes budget tools and F-05 triggers trustworthy |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| ContextManager copy-on-write history with snapshots | **kept (adapted)** | orchd `TranscriptManager` gains Arc-shared snapshots; hostd session tree remains the durable store (no second durable copy) |
| Per-message token accounting | **kept** | orchd transcript domain tracks estimates incrementally with one documented estimator; preflight consumes the tracked snapshot |
| Function-output truncation with markers | **kept** | normalized model view truncates oversized tool results with an explicit marker; committed transcript untouched |
| `get_context_remaining` / `new_context_window` tools | **rejected (this slice)** | the accounting lands first; tools are a follow-on F-04 slice once F-05 defines budget windows |
| Pre-sampling auto-compact / budget windows / remote compaction | **rejected (this slice)** | owned by F-05; orchd keeps failing closed instead of guessing hostd policy |

## Open questions

1. Whether the truncation cap should become a user-facing `[transcript]`
   setting (default 24k tokens) or stay derived from the resolved model's
   window — deferred to F-05 budget-window work.

## Reference evidence

- codex-rs `core/src/context_manager/history.rs` (copy-on-write history),
  `normalize.rs` (projection), `updates.rs` (in-place updates),
  `tools/context.rs` (result truncation).
- piko `packages/orchd/src/domain/transcript/` (pre-slice manager),
  `packages/orchd/src/runtime/execution/budget.rs` (pre-slice preflight),
  `packages/orchd/src/runtime/execution/actor.rs` (model-step dispatch),
  `packages/hostd/src/domain/compaction/mod.rs` (hostd-side estimation).

## Slice 2: World-state diffing across turns

### Summary

Run identity facts (`state.run` from F-03) stop being rebuilt into the frozen
per-run prompt. They become a retained, data-only **world-state Context
message** injected into the model-visible transcript at run start: the first
run of a session injects the full snapshot, and each continuation run injects
only the facts that changed since the previous run's frozen baseline. hostd
owns the durable baseline and the full-vs-diff decision; orchd commits and
retains the message before the turn's user message. Transcript rewrites
(compaction) clear the baseline so the next run falls back to a full
re-injection. Implementation: [D-17](../design/D-17-world-state-diffing.md).

### Problem

F-03 freezes a full `state.run` snapshot into every run's prompt. Because the
frozen prompt is rebuilt per run and its dynamic-context blocks are not
retained in the transcript, the model re-reads the same identity facts every
turn. That defeats the codex-rs "freeze once, diff later" model: as
world-state grows (permissions, environments, agent roles in later
milestones), re-stating the full snapshot every run wastes context and makes
the prompt noisier, while nothing tells the model what actually changed.

### User journeys

1. An agent starts a fresh session. The model sees one full world-state
   message (session, agent, operation, run kind, model) before the first
   user message.
2. The session continues for a second turn. The model sees the same full
   snapshot retained from turn 1, plus a short update naming the new
   operation id (and any other changed facts) right before turn 2's user
   message — not a repeated five-line block.
3. A user compacts a long session. The next run re-injects the full snapshot
   so the model is never left with a diff and no baseline.

### In scope

- hostd-owned world-state fact set and durable per-session baseline
  (session manifest, mirroring `last_model`).
- Full-vs-diff content computation against the previous run's frozen facts;
  fixed key order; `<unset>` for removed facts; no message when nothing
  changed.
- orchd run-start injection: the world-state Context message is committed
  before the input message (linear parent chain) and pushed into the
  execution transcript, so it is retained in the session transcript.
- Baseline clearing on successful compaction (auto `Summarize`, manual
  `session.compact`, and `new_context_window` fresh window).
- Removal of the `state.run` block from the frozen prompt catalog and the
  assembly-version bump (`3 → 4`).

### Out of scope

- Diffing for `environment.host` / `environment.run` (they stay RunDynamic
  prompt blocks; host facts are small and per-run).
- `context.model-switch` behavior (unchanged; it already reports model
  changes separately).
- Subagent (F-10) world-state injection for child agent runs; this slice
  covers hostd root turns only.
- Token-budget context fragments (separate M1 follow-on).
- Rollout/world-state patch records (codex-rs `WorldStateItem::patch`); the
  durable transcript message is the record.

### Behavior and states

#### Facts and baseline

- Fact set and fixed order: `session_id`, `agent_instance_id`,
  `operation_id`, `run_kind` (`initial` | `continuation`), `model`. All
  facts are optional except `run_kind`, which is always known.
- hostd records the current facts as the new baseline immediately when a
  turn is accepted (in-memory `SessionState` + durable manifest), returning
  the previous baseline for the full/diff decision — the same pattern as
  `record_turn_model` / `last_model`.

#### Full vs diff emission

- **Full** (no baseline): one line per available fact in fixed order,
  byte-identical to the F-03 `state.run` block content.
- **Diff** (baseline exists): a header line
  `world-state changed since the previous run:` followed by one line per
  changed fact in fixed order (`fact: value`); a fact that became
  unavailable renders `fact: <unset>`; unchanged facts render nothing.
- The diff message is emitted only when at least one fact changed.

#### Model-visible shape and retention

- The world-state message is `Message::Context` (data-only; authority=None,
  trust=Trusted, source `run-state` / `hostd/session`), pushed into the
  execution transcript before the turn's user message and committed to the
  durable AgentInstance shard before the input commit.
- Because the message is committed to the session transcript, it survives
  into later runs: a continuation sees the retained full snapshot (from the
  first run) followed by each run's diff lines.
- The frozen run prompt no longer contains a `state.run` block;
  `AGENT_RUN_PROMPT_ASSEMBLY_VERSION` bumps `3 → 4`.

#### Baseline invalidation

- Every successful compaction rewrite (auto Summarize, manual compact, and
  `new_context_window`) clears the durable + in-memory baseline, so the next
  run re-injects the full snapshot (mirrors codex-rs clearing
  `world_state_baseline` when history is rewritten).
- After a hostd restart, continuation runs keep diffing against the
  restored manifest baseline.

### Acceptance criteria

- [x] A fresh session run injects one full world-state Context message
      before the user message, with all available facts in fixed order
      (unit evidence).
- [x] A continuation run injects only the changed facts before its user
      message; unchanged facts are absent from the diff and remain visible
      through the retained full snapshot from the first run (unit +
      end-to-end evidence).
- [x] An unchanged fact set emits no world-state message (unit evidence).
- [x] A fact that becomes unavailable renders `<unset>` (unit evidence).
- [x] The durable baseline survives a hostd restart: the next continuation
      diffs against the restored baseline instead of re-emitting full
      (storage evidence).
- [x] A successful compaction (auto/manual/new-context-window) clears the
      baseline; the next run re-injects full (integration evidence).
- [x] The frozen prompt contains no `state.run` block; the assembly version
      is 4; environment blocks remain RunDynamic and never change the
      stable prefix digest (regression guard).
- [x] The durable transcript chain is linear: head → world-state → input;
      the world-state commit precedes the input commit (integration
      evidence).
- [x] Differential validation: a two-run session shows full → diff emission
      mirroring codex-rs `update_world_state` full-then-patch behavior at
      the message level.

### Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Who owns the baseline and diff decision? | hostd (durable manifest + in-memory `SessionState`) | hostd is authoritative for user-visible state; orchd stays a transient executor (ADR-002) |
| Where does the model see world-state? | A retained transcript Context message, not the frozen prompt | Retention across runs is impossible from the per-run frozen prompt; transcript Context messages are already model-visible and durable |
| When is full vs diff emitted? | Baseline absent → full; baseline present → diff | Mirrors codex-rs `update_world_state`; stale baselines after history rewrites are prevented by clearing on compaction |
| What does a diff look like? | `fact: value` lines with a header; `<unset>` for removals | Line format stays consistent with F-03 content; the header marks the message as an update |
| What happens on compaction? | Clear the baseline; next run re-injects full | Matches codex-rs clearing `world_state_baseline` on history rewrite so diffs never reference a lost snapshot |

### Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| `update_world_state`: snapshot, render diff fragments, persist baseline | **kept (adapted)** | piko diffs the F-03 fact set in hostd and injects one transcript Context message per run; no per-section fragment rendering (piko has a single flat fact set today) |
| `world_state_baseline` cleared on history rewrite | **kept** | piko clears the hostd manifest baseline on every successful compaction |
| `WorldStateItem::full/patch` rollout records | **rejected (this slice)** | piko has no rollout record for world-state; the durable transcript message is the model-visible and durable record |
| JSON merge-patch diff (RFC 7386) | **rejected (adapted)** | piko's fact set is flat and line-rendered; a line diff is smaller and matches the existing block format |

### Open questions

1. Subagent runs (F-10): world-state injection is root-turn-only today;
   per-agent baselines would be needed if child agents get run identity.

### Reference evidence

- codex-rs `core/src/context_manager/history.rs` (`update_world_state`,
  baseline clearing on history rewrite)
- codex-rs `core/src/context/world_state/mod.rs` (snapshot, merge-patch,
  diff rendering)
- piko `packages/hostd/src/application/turns/submit.rs` (run assembly),
  `packages/hostd/src/application/compaction.rs` (baseline clearing),
  `packages/hostd/src/infra/storage/session_store/` (durable manifest +
  message commits)
- piko `packages/orchd/src/runtime/execution/{mod,actor}.rs` (run-start
  commit ordering)
- piko `packages/llmd/src/executor/prompt_mapping.rs` (Context → user-role
  model message)
