# Turn Lifecycle and Live Projection

Selected feature contract: [Turn Lifecycle](../features/turn-lifecycle.md).

The full cross-crate design and rollout contract is maintained in
[`docs/turn-lifecycle-and-live-projection.md`](../../../../docs/turn-lifecycle-and-live-projection.md).
This document records the TUI-facing implementation boundary.

## Responsibilities

- orchd owns long-lived Task handles and per-input Work execution.
- hostd owns Turn identity, durable terminal outcomes, live session projection, and client notifications.
- the TUI owns only presentation state for the active turn.

## Data flow

hostd allocates a unique turn and root work identity before submission. orchd reports Work and Task lifecycle using that identity. A successful turn requires the matching root Work to be terminal and the matching root Task to be stable. hostd durably records the Turn transition, updates HostState, and only then publishes the client lifecycle event.

Committed transcript notifications are projection-visible notifications. The TUI receives their complete payload from HostState; live observation does not reread an actively appended task shard.

## State ownership

The TUI caches the current turn identity only to render existing running state. `TurnStarted` sets it. Matching completed, failed, or cancelled events clear it. A rejected submit command also clears the provisional identity. Session hydration replaces the cache from the hostd snapshot.

hostd records Turn lifecycle separately from task shards so host-owned records do not consume orchd task sequence numbers. On reopen, a durable non-terminal Turn without a provably live execution is finalized as interrupted instead of being projected as running.

## Layout and focus

No slot, panel placement, focus target, input priority, or overlay lifecycle changes are introduced.

## Protocol constraints

Turn lifecycle events retain `turn_id`, `root_task_id`, and `root_work_id` where known. Root Work notifications must carry the same `source_turn_id`. Conflicting terminal outcomes or identity bindings are protocol errors; identical terminal replay is idempotent.
