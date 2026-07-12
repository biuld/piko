# Invariants

> Status: current  
> Audience: orchd contributors

Rules that must hold across API, runtime, persistence, and observation. Violations are bugs, not edge cases.

## Identity and routing

1. **Task is the unit of execution.** All runtime routing, persistence, and observation use `task_id`.
2. **`agent_id` is metadata only.** It must never be used as a storage or routing key.
3. **One task → one JSONL shard.** Multiple tasks may share an `agent_id` but never a file.
4. **`parent_message_id` is intra-task.** Cross-task references use spawn/steer audit fields, not transcript parent links.

## Input and transcript

5. **Single commit path.** All user input (root, follow-up, steer, queue) goes through `commit_input` + PersistSink.
6. **No LLM before ack.** A model step must not start until `commit_message` returns `PersistAck`.
7. **Transcript append after ack.** In-memory transcript updates only after durable commit succeeds.
8. **Idempotent input.** Same `request_id` returns the same receipt; same `message_id` never appends twice.
9. **Lifecycle is not transcript.** `TaskEvent::Created.prompt` and similar fields are notification/audit only — not recovery sources.

## Persistence

10. **Monotonic `task_seq`.** Every durable fact on a task gets the next sequence number; gaps are errors on replay.
11. **Barrier ≠ enqueue.** Ordering internal persist events without awaiting ack does not satisfy the write-before-LLM rule.
12. **Commit failure is fail-closed.** No transcript append, no step start, API returns `PersistenceFailed`.
13. **Shard completeness.** All message types for a task live in that task's shard with required identity fields.
14. **Session-scoped sink.** One `PersistSink` instance per open `session_id` in hostd; turns reuse the same `Arc`, never construct a per-turn sink for production paths.

## Observation

15. **Events after commit.** `SessionEvent::MessageCommitted` (and tool equivalents) publish only after PersistAck.
16. **Deltas are best-effort.** Realtime deltas may drop; clients reconcile on committed events.
17. **No global Event/Delta order.** Do not assume `MessageEnded` precedes `MessageCommitted`.
18. **Session-scoped hub.** One hub per `session_id`; subscription survives task idle/close.
19. **Observation ≠ state input.** Session events notify; they do not drive supervisor or hostd state machines. HostState updates for committed messages happen at the persistence barrier, not in the observation handler.
20. **Observation reads shards.** `MessageCommitted` / `ToolCommitted` handlers load payload from `TaskRepository`; they must not rely on per-turn in-memory sink caches.

## Control and lifecycle

21. **CancelWork scope.** Cancels current work only — not the task, turn, or sibling tasks.
22. **Task survives work failure.** Failed work leaves the task resumable via new input.
23. **Close vs terminate.** Close rejects input temporarily; terminate ends the handle permanently.
24. **Runtime registry owns handles, not transcript.** Live task handles are tracked internally; transcript lives in shards + in-memory task state.

## API boundaries

25. **hostd uses public API only.** `orchd-api`, `orchd::Runtime`, and `orchd::api` — not internal runtime modules.
26. **Command ack separate from observation.** `InputReceipt` / `TaskHandle` are not transcript entries.

## Verification

These invariants are enforced by:

- Integration tests under `packages/orchd/tests/` (see [testing.md](testing.md))
- hostd session storage tests for shard layout and recovery
- Code review on any change touching `commit_input`, PersistSink, or SessionOutputHub

When adding features, identify which invariants apply and add or extend tests for the risky ones.

## Related reading

- [task-runtime.md](task-runtime.md)
- [persistence.md](persistence.md)
- [events-and-observation.md](events-and-observation.md)
- [host-integration.md](host-integration.md)
- [turn lifecycle and live projection](../../../docs/turn-lifecycle-and-live-projection.md) — session sink lifecycle, barrier vs observation, and terminal Turn convergence
