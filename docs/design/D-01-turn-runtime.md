# D-01: Turn-runtime slices — admission, abort markers, typed tasks

> Status: implemented
> Implements: [F-01](../features/F-01-turn-runtime.md)
> Decisions: product decisions live in the F-01 PRD (fixed queue cap,
> steer-active rejection, fixed model-visible abort marker)

## Goal

Close the three remaining F-01 gaps:

- **A. Input admission** — a fixed-cap follow-up queue that returns overload
  instead of growing without bound.
- **B. Turn-abort markers** — a durable, model-visible marker at the
  interruption point, both on live cancellation and on crash recovery.
- **C. Typed background tasks** — task kinds, typed results, cancellation, and
  session-scoped lifecycle so shutdown never orphans work.

## Constraints and non-goals

- Transcripts stay append-only and deterministic; marker message ids are stable
  and derived from the run, never random.
- Overload is a bounded, retryable signal; it never drops queued work.
- Compact, review, and user-shell task *kinds* remain placeholders owned by
  F-05, F-11, and F-08; F-01 ships the task infrastructure only.
- No changes to the model gateway (Context messages already render
  model-visible with `authority=None`) or to the tool system (F-06).
- Session storage schema stays v3; no new per-task storage files.
- Rust files stay under the 500-line ceiling; oversized modules split into
  submodules.

## Proposed design

### A. Fixed-cap follow-up queue

Owner: the agent actor (`AgentActor`). `enqueue_follow_up` enforces a fixed
cap (`MAX_QUEUED_FOLLOW_UPS`, a module constant) *before* the durable
`InputQueued` commit: when the queue is full, the call returns overload and
nothing is written.

- Overload propagates as `Overload` to the `run_agent` caller; the hostd submit
  path marks the turn failed with an overload message (reuses the existing
  `TurnStatus::Failed` mapping).
- Idempotency and cancel semantics are unchanged. Cancelling a queued input
  frees its slot.
- The cap applies to the durable queue length, so recovery cannot resurrect a
  queue past the cap.

### B. Turn-abort markers

The marker is a `Message::Context` (data-only, trusted runtime context):

- Content: fixed guidance — "The previous turn was interrupted on purpose. Any
  tools or commands that were aborted may have partially executed."
- `trust: Trusted`, `source: PromptSource { kind: "turn_aborted",
  locator: "<execution_id>" }`.
- Stable message id: `"{execution_id}/abort_marker"`.

The gateway already maps Context messages to a model-visible user-role message
with `authority=None`, so the marker reaches the next run's prompt without
carrying user instruction authority.

Two insertion points:

1. **Live cancellation.** The `ExecutionActor` commits the marker through the
   existing `MessageCommitScope` when a run reaches a cancelled outcome,
   *before* assembling the terminal transcript. The cancelled terminal then
   contains the marker, and every later run sees it exactly once.
2. **Crash recovery.** `interrupt_incomplete_agent_executions` (hostd storage)
   already rewrites runs that never reached terminal into cancelled reports at
   session open. Extend it to append the same marker message to the
   interrupted agent's durable transcript with the stable id before the
   manifest is rewritten. Repeat recovery is idempotent because the marker id
   is stable.

Only cancelled runs get markers; failed runs already carry their error in the
terminal outcome.

### C. Typed background tasks

New orchd module `runtime/tasks/` (orchd-internal; no protocol DTOs):

- `TaskRegistry`, owned per session by `SessionAgentScope`: `spawn(kind,
  owner, cancellable future) -> TaskHandle`, `cancel(task_id)`,
  `cancel_all()`, and snapshot access.
- `TaskSnapshot` — task id, feature-owned `kind` (open string), owning agent
  instance, status, start/finish times, summary, and an optional typed result
  or error.
- Every task holds a `CancellationToken`; `cancel_all()` runs on session
  detach/shutdown (`scope.shutdown()`), so no orphaned work survives a
  session close.
  Shutdown marks running tasks cancelled.

Task kinds are deliberately **not** enumerated here: piko's roadmap features
already own their durable channels, so a codex-rs-style central task taxonomy
(`regular`/`compact`/`review`/`user_shell`) would be speculative coupling.
Compaction lives in hostd (summarizer), user-shell results are tool results in
the transcript, and review results land in the inbox. Each feature defines its
own `kind` string and persists results through the channel it already owns
(transcript messages or inbox reports) when it lands (F-05, F-08, F-11); F-01
introduces no new durable surface.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | none |
| `piko-orchd` | Follow-up cap; abort marker on live cancel and startup cancel; `runtime/tasks` registry |
| `piko-hostd` | abort marker append during recovery |
| `piko-llmd` | none |
| `piko-sandbox` | none |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Overload: the caller retries; queued work is untouched and the queue length
  is unchanged.
- Marker commit failure on cancel: the run fails closed (terminal is Failed
  with the persistence error) so the abort is never silently dropped; the
  marker is never duplicated because its id is stable.
- Task cancel: the cancellation token aborts in-flight work (cooperative via
  the token, and the select branch drops the future) and the snapshot marks
  `cancelled`.
- Session detach mid-task: `cancel_all()` aborts and marks tasks cancelled;
  attach never resurrects running tasks.

## Verification

- Orchd unit tests: queue cap boundary and overload; live-cancel and
  startup-cancel markers in the committed transcript; task lifecycle
  (succeeded/failed/cancelled) and `cancel_all` on session shutdown.
- Hostd integration tests: recovery appends the marker to an interrupted
  agent's transcript idempotently.
- Differential: marker wording against codex-rs `TurnAborted` guidance; the
  task registry's behavior (not its taxonomy) mapped to the F-01 acceptance
  criteria.

## Alternatives considered

- Unbounded queue with soft backpressure: rejected — the PRD requires bounded
  memory and a deterministic overload signal.
- Marker as an assistant or user message: rejected — Context keeps
  `authority=None` and an explicit trust level so the marker is data, not
  instruction or fabricated model output.
- A codex-rs-style task taxonomy plus a new durable task surface: rejected —
  piko's features already own their durable channels (transcript, inbox), and
  a central taxonomy would couple protocol and storage to work that has no
  consumer yet (ADR-001: behavior first, architecture is not translated).

## Rollout

All slices landed (2026-08-02):

1. Follow-up queue cap + tests.
2. Live-cancel and startup-cancel abort markers + tests.
3. Recovery abort marker + hostd tests.
4. `runtime/tasks` registry with open kinds, typed results, session-scoped
   cancellation, and tests.
