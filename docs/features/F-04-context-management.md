# F-04: Context management — transcript accounting, snapshots, and output truncation

> Status: implemented
> Priority: P0
> Source evidence: codex-rs `core/src/context_manager/{history,normalize,updates}.rs`,
> `core/src/context_manager/token_budget.rs`, `core/src/tools/context.rs`
> (tool-result truncation)

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

- [ ] Per-message token estimates are tracked by the transcript manager; the
      running total equals the sum of estimates and stays consistent across
      push and rollback (unit evidence).
- [ ] Two consecutive `snapshot()` calls share one allocation
      (`Arc::ptr_eq`); any mutation invalidates the cache and produces a new
      snapshot (unit evidence).
- [ ] A tool result above the cap is truncated in the model view with an
      explicit marker containing retained/total counts; small results,
      image blocks, and error metadata pass through; the committed
      transcript retains the full output (unit + end-to-end evidence).
- [ ] The budget preflight accounts the normalized model view and reports
      `context_remaining` on rejection; the estimate basis is identical to
      what is dispatched to the gateway (unit evidence).
- [ ] End-to-end: a tool emitting output above the cap yields a truncated
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
