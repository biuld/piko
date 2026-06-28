# hostd Global Plan

This document is the current planning source for `hostd`.

It replaces older gap/status reports that were written before the protocol crate,
session commands, queue events, OAuth, MCP, and compaction paths were partially
implemented. When code and this document disagree, verify the code first.

## Target Architecture

```text
host-tui (TypeScript)
  <-> JSON-lines stdio
  <-> hostd (Rust host runtime)
  -> orchd (Rust orchestrator library)
  -> sandbox / tools / model gateway
```

`hostd` is the product authority for user-visible runtime state:

- sessions, branches, metadata, snapshots, and recovery
- turn lifecycle and cancellation
- prompt expansion, context files, skills, and system prompt assembly
- model/auth/settings resolution
- approvals and queue state
- transcript persistence and compaction
- adapter logic from orchestrator runtime facts to TUI-facing events

`orchd` owns agent execution, task orchestration, model steps, and tool routing.
The TUI owns presentation state only.

## Protocol Ownership

The Rust protocol source of truth is now `packages/protocol`.

```text
packages/protocol/src/command.rs  -> Command, CommandAck
packages/protocol/src/event.rs    -> Event and snapshots
packages/hostd/src/api.rs         -> pub use piko_protocol::*
packages/host-tui/src/client/hostd-protocol.ts -> hand-maintained TS mirror
```

Near-term protocol work:

- Generate the TypeScript mirror from `packages/protocol` or add a parity test.
- Decide whether `CommandAck` means parse/acceptance only or semantic execution
  acceptance. Today JSONL writes `command_accepted` before command execution.
- Either implement event replay for `events_resume(after_seq)` or rename the
  command semantics to snapshot recovery. Today resume returns a snapshot.
- Keep `HostEvent`/`Event` naming reserved for the TUI-facing protocol. Internal
  orchd events should not be named HostEvent.

## Current Functional State

The following areas are already wired in current code:

| Area | Current state |
|---|---|
| JSON-lines server | Commands, acks, event stream, stdio hostd binary |
| Session storage | JSONL create/open/list/fork/import/rename/delete/navigate/snapshot |
| Turn execution | `TurnRunner` abstraction plus `OrchTurnRunner` over `OrchCore` |
| Prompt resources | Context files, prompt templates, system prompt, project/global skills |
| Settings | Layered settings, runtime `config_set`, project settings persistence |
| Auth/model | API key storage, OAuth credential resolution, device-code command path, model catalog/list |
| Queue commands | steer/follow-up/next-turn commands and `queue_update` events |
| Approval commands | `approval_requested`, `approval_respond`, `approval_resolved` path exists |
| MCP | stdio JSON-RPC client path and tool registration exist |
| Compaction | token estimates, threshold/cut-point logic, LLM summary path, compaction entry append |

These are not the same as production-complete. Several paths are present but
need concurrency, state, and protocol hardening.

## Critical Fixes

### P0: Do not hold `HostState` across a turn

`apply_turn_submit()` currently locks `HostState` and awaits `runner.run_turn()`.
That blocks concurrent commands that need the same state lock, including
approval response, cancellation, and queue commands.

Target:

- Build the turn input while holding the lock.
- Persist the user message and set active turn state.
- Release `HostState` before awaiting orchd/model/tool work.
- Re-acquire state only to apply completed domain events, update usage, persist
  transcript entries, or drain queues.
- Add a regression test where a runner waits for approval and
  `approval_respond` completes while the turn is still active.

### P0: Make turn lifecycle single-authority

`apply_turn_submit()` emits `turn_started`, and `OrchTurnRunner` also emits
`turn_started`. This produces duplicate stream starts and inconsistent
`root_task_id`.

Target:

- `hostd` owns turn lifecycle events.
- `orchd` owns task/message/tool/approval events.
- Root task binding should come from the actual `task_created` event or a
  deterministic root task id passed through the runtime contract.
- Add an integration test that one `turn_submit` produces exactly one
  `turn_started` and one `turn_completed`.

### P0: Cancellation must reach the running task

`turn_cancel` updates host state but does not currently prove cancellation
reaches the running orchd task. It also contends with the state lock issue.

Target:

- Track `session_id -> active turn -> root task ids`.
- Route cancel to orchd before or while changing host state to cancelling.
- Emit final `turn_cancelled` only after the runner settles or cancellation is
  acknowledged.

### P1: Normalize command failure semantics

The JSONL server accepts a parsed command immediately. Later failures become a
synthetic `task_failed` with empty `session_id`.

Target:

- Reject commands that fail validation before side effects.
- For accepted long-running commands, emit typed domain failure events with
  correct `session_id` and `turn_id`.
- Avoid command failures represented as unrelated task failures.

### P1: Make queue state coherent

Queue commands and `queue_update` exist, but steering can be both delivered
immediately and left in `steer_queue`. Follow-up and next-turn draining is also
coupled to recursive `apply_turn_submit()`.

Target:

- Define queue semantics explicitly:
  - `steer`: delivery to a running task, not a durable follow-up prompt.
  - `follow_up`: run after the active turn.
  - `next_turn`: run after all follow-ups or at the next idle opportunity.
- Emit `queue_update` after enqueue and after drain/delivery.
- Keep queue draining outside recursive turn submission if possible.

### P1: Fix resume and seq model

`SessionState.seq` advances on persisted entries, but `Event` does not carry
seq and `events_resume(after_seq)` returns a snapshot.

Target:

- Choose one model:
  - snapshot-only recovery, with docs and command names matching that, or
  - real event replay with event seq/cursor.
- Do not leave `after_seq` as a misleading no-op.

### P1: Protocol parity automation

The TS mirror is hand-maintained and already has at least one classification
risk: Rust treats `model_listed` as domain, while the TS helper domain set has
omitted it before.

Target:

- Generate TS types from Rust, or
- Add serialization fixtures and a parity test for every command/event variant,
  including domain/streaming classification.

## Product Parity Work

After the P0/P1 runtime fixes, remaining product work should be planned by
surface area rather than by old TS-vs-Rust gap lists.

| Area | Work |
|---|---|
| Session UX | clone/switch semantics, session list metadata, previews, modified times, message counts |
| Auth UX | browser/device-code guidance, provider-specific OAuth polish, error recovery |
| Approval policy | session/workspace scoped approvals, auto-accept rules, persisted approval policy |
| Skills | skill execution as a first-class run mode, metadata overrides for model/thinking/tools |
| Compaction | deterministic tests, summary quality, branch summary policy, snapshot behavior after compaction |
| MCP | lifecycle supervision, restart/error reporting, tool discovery diagnostics |
| Multi-agent UI | child task transcript projection, task tree state, plan/progress state |
| Observability | structured logs, command/event tracing, runtime timing, debug dumps |

## Suggested Implementation Order

1. Fix `HostState` lock lifetime and approval/cancel concurrency.
2. Remove duplicate `turn_started` and make turn/task ownership explicit.
3. Define command ack/failure semantics and update JSONL tests.
4. Decide snapshot-only vs replay recovery; update protocol and TUI client.
5. Normalize queue semantics and tests.
6. Add Rust/TS protocol parity checks or codegen.
7. Harden compaction, MCP, OAuth, and multi-agent UI one surface at a time.

## Documentation Rules

- Do not create new broad "gap analysis" docs without linking this file.
- Avoid event-count claims unless generated or checked against
  `packages/protocol/src/event.rs`.
- Treat older migration reports as historical notes only.
