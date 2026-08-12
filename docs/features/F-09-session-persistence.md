# F-09: Session persistence

> Status: superseded in storage/replay by F-31; residual list paging / prewarm
> deferred
> Priority: P1
> Source evidence: codex-rs `core/src/thread_manager.rs`,
> `thread_manager_tests.rs`, `rollout.rs`, external `codex_thread_store`;
> digest Block H; piko hostd schema-v3 storage and TUI `session.fork`
>
> [F-31 durable session journal](F-31-durable-session-journal.md) replaces the
> schema-v3 storage/replay contract. F-09 retains only deferred session-list
> paging and startup prewarm scope.

## Summary

Sessions are durable, reopenable stores of conversation history and agent
metadata. Users can resume work after host restarts, list sessions for a
working directory, clone a full session into a new file, and **fork from a
chosen history entry into a new independent session** whose transcript stops
at that branch point. Crashed or unrecovered in-flight work does not leave
phantom running state on reopen.

## Problem

Without a host-owned durable session model, turns and tools cannot be
resumed, forked, or audited. Clients already expose fork/clone and tree
navigation; a branch-point fork is part of the user contract when the tree
selection is the start of a new alternative *file*, but hostd currently
rejects `SessionFork` with `entry_id` set. Long multi-branch sessions need
a way to keep one lineage as a new session without copying abandoned siblings
or post-branch work.

## User journeys

1. The user restarts the client and opens an existing session by id or path.
   Hostd rehydrates transcript, agents, and current leaf; any turn that was
   only in memory without a live execution is finalized; incomplete agent
   executions are cancelled with durable abort markers (F-01).
2. The user clones the current session (`SessionFork` without `entry_id`).
   A new session id and directory appear with the **full** history and agent
   state of the source; the source is unchanged.
3. The user selects a tree entry and runs fork with that entry (or
   `/fork <entry>`). A **new** session opens whose retained history is the
   ancestor path through that entry only. Continuing the new session does not
   rewrite the source. Sibling branches and descendants past the entry are
   absent from the forked session.
4. The user navigates within the same session (tree navigate / `SessionNavigate`).
   History is retained in one file (Leaf / optional branch summary); this path
   remains separate from fork-into-new-file.
5. The user lists sessions for a cwd and opens one from the list.

## In scope

### Baseline (already implemented; documented here)

- Schema-v3 layout: `session.json` manifest + per-agent JSONL shards under
  `~/.piko/sessions/<encoded-cwd>/<session-id>/`.
- Create, open/resume, rename, delete, import; `SessionList` for cwd/global.
- Full-session clone fork (`entry_id` absent).
- On open: finalize interrupted host turns; interrupt incomplete agent
  executions and append F-01 turn-abort markers when missing.
- In-session navigate (`SessionNavigate`) with optional branch summary.
- Compaction-aware load of active branches (F-05) and world-state baseline
  (F-04).

### Slice 1 — branch-point fork

- `SessionFork { session_id, entry_id: Some(id) }` creates a new durable
  session whose retained entries are the **single root path through `id`**
  (ancestors of `id` plus `id` itself), in stable chronological order.
- Source session is never modified.
- Forked session gets a new `session_id`, new directory, fresh
  `created_at` / `updated_at`, and `current_leaf_id = entry_id`.
- Agent shards in the fork only retain messages on that path; only agent
  instances still referenced by retained messages (always including the root
  agent, if any) appear in the forked manifest.
- Transient multi-agent runtime queues are not copied: empty
  `agent_inbox`, `agent_input_queue`, and `agent_executions` on the fork
  (full clone applies the same clear for symmetry).
- World-state baseline is **cleared** on branch-point and full clone fork so
  the next root turn re-injects a full F-04 snapshot against the kept
  transcript (closes the F-04 open question on baseline mismatch after fork).
- Reject with a clear error when `entry_id` is unknown, or when the source
  has an active hostd turn (same class of fail-closed as navigate).

**Implementation status:** landed (D-26 / V-26).

## Out of scope

- Cursor-paged session list APIs beyond today’s full list (codex thread-list
  paging).
- Session startup prewarm of model connections.
- Cross-cwd automatic fork when resuming another project’s session.
- In-session navigate redesign; Leaf graph remains the in-file branching model.
- Export/import format beyond directory import.
- Merging two forked sessions back together.
- Soft-links / shared storage between source and fork (fork is a deep copy of
  the retained slice only).

## Behavior and states

### Branch-point resolution

Given source entries (manifest metadata union agent transcript projections)
and `entry_id`:

1. Fail if no entry has that id.
2. Retained set = ancestors walk from `entry_id` via `parent_id` through the
   tree (same lineage helper as compaction’s active branch, anchored at
   `entry_id`).
3. Include every retained entry; exclude all other entries (siblings,
   abandoned branches, and future descendants).

User messages selected as the branch point **include** that user message in
the fork (unlike navigate, which may move the leaf to the parent and fill the
editor). The model-visible transcript in the fork ends at that point;
the user continues by submitting new input in the forked session.

### Agent and inbox handling

- Root agent identity is always retained when present in the source.
- Child agent instances with **no** retained transcript messages on the path
  are omitted from the forked session.
- Inbox items, queued inputs, and non-terminal execution records are cleared
  so the fork cannot resume half-open multi-agent work from the source.

### Full clone vs branch fork

| Mode | `entry_id` | History | Agents / shards |
|---|---|---|---|
| Clone | absent | all entries | all agents, full shards |
| Branch fork | present | path through entry only | referenced agents; filtered shards |

Both clear world-state baseline. Both assign a new session id.

### Errors

| Condition | Result |
|---|---|
| No storage backend | fail command |
| Unknown `session_id` | session not found |
| Unknown `entry_id` | invalid command (clear message) |
| Source has active turn | `ActiveTurnExists` (or equivalent fail-closed) |
| Empty/corrupt source | storage invalid |

## Acceptance criteria

- [x] Full-session clone (`entry_id` absent) still creates an independent copy
      of the entire source history and agent metadata (minus cleared baseline
      and no live executions).
- [x] Branch-point fork retains only the path through `entry_id`; sibling and
      post-entry history are absent from the forked session’s reconcile view.
- [x] Source session leaf, entries, and files are unchanged after either fork.
- [x] Forked session opens like a normal open (reconcile + agents) with
      `current_leaf_id` equal to the branch-point entry.
- [x] World-state baseline is unset on the forked session so the next root
      turn injects a full world-state Context when F-04 applies.
- [x] Unknown `entry_id` and active-turn source fail closed without writing a
      partial fork directory.
- [x] Incomplete crash recovery still appends F-01 abort markers / cancels
      non-terminal agent executions on open (regression).

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Branch fork vs in-session navigate | Separate commands | Navigate keeps one file and all history; fork creates a new durable session |
| Include user message at branch point? | Yes | Matches “fork from this message”; navigator’s parent/editor rule is navigate-only |
| Copy multi-agent inbox / running executions? | No | Fail-closed; fork is a static history cut, not a live scheduler snapshot |
| World-state baseline on fork | Clear | Avoids F-04 diff against a full snapshot that is no longer on the path |
| Message ids in fork | Preserve source ids for retained messages | Parent chains stay valid; new work allocates new ids |
| Schema change | None (v3) | Filter existing records; no migration |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Thread store with metadata + rollouts | **kept (adapted)** | hostd schema-v3 session dir + JSONL; hostd authoritative |
| Fork / branch thread | **kept (adapted)** | full clone already landed; branch-point fork is this slice |
| Thread graph sections | **rejected (adapted)** | piko `SessionTreeEntry` parent graph + Leaf navigation |
| Rollout cursor paging | **deferred** | no piko list-paging consumer at scale |
| Interrupted-turn markers | **kept (adapted)** | F-01 abort markers + host open finalization + incomplete execution sweep |
| Session startup prewarm | **deferred** | no critical path consumer |

## Open questions

1. Cursor-paged `SessionList` when project session counts grow large.

## Reference evidence

- codex-rs `core/src/thread_manager.rs`, `thread_manager_tests.rs`
- piko TUI `/fork`, `/clone`, session tree (separate navigate)
- piko `packages/hostd/src/infra/storage/jsonl_repository/fork_import.rs`
- piko `packages/hostd/src/application/sessions/mutate.rs` (`apply_session_fork`)
- piko `packages/hostd/src/domain/compaction/tree.rs` (`active_branch_entries`)
- F-01 turn-abort reconstruction; F-04 world-state baseline; F-05 compaction
