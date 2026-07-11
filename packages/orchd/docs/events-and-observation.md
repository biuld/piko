# Events and Observation

> Status: current  
> Audience: both

orchd separates **durable facts**, **session observation output**, **runtime lifecycle**, and **command acknowledgement**. They must not be conflated.

## Planes

| Plane | Purpose | Consumer |
|---|---|---|
| Durable facts | JSONL, recovery, audit | hostd `PersistSink` |
| Session observation | Live UI + reliable notifications | hostd / TUI via `SessionSubscription` |
| Runtime lifecycle | Supervisor live registry | Internal lifecycle observer |
| Command acknowledgement | API caller feedback | `TaskHandle`, `InputReceipt`, etc. |

## SessionOutput

Public observation uses one stream with two QoS lanes:

```rust
pub enum SessionOutput {
    Event(SessionEventEnvelope),   // reliable
    Delta(RealtimeDeltaEnvelope),  // best-effort realtime
}
```

Merged at the subscription boundary; internally the hub keeps separate lanes so high-frequency token deltas cannot block reliable events.

### Reliable events (`SessionOutput::Event`)

Published **only after** the corresponding durable commit succeeds.

```rust
pub enum SessionEvent {
    TaskChanged { snapshot: TaskSnapshot },
    WorkChanged { snapshot: WorkSnapshot },
    MessageCommitted { message_id, work_id, role },
    ToolCommitted { message_id, work_id, tool_call_id },
    InteractionRequested { request },
    InteractionResolved { resolution },
}
```

Guarantees:

- Ordered by `task_seq` within a task.
- Session hub assigns a runtime-scoped cursor for retention and resubscribe.
- Slow subscribers must not block durable commit or LLM execution.
- When retention is exhausted or cursor epoch mismatches, yield `SnapshotRequired`.

### Realtime deltas (`SessionOutput::Delta`)

For streaming UI only — not persisted, not used for recovery.

```rust
pub enum RealtimeDelta {
    MessageStarted { role },
    Text { content_index, delta },
    Thinking { content_index, delta },
    ToolCall { content_index, tool_call_id, delta },
    MessageEnded { stop_reason, error_message },
}
```

Guarantees:

- May be dropped under subscriber lag.
- `delta_seq` orders deltas within one message only.
- Clients treat `MessageCommitted` as authoritative and use it to correct temporary deltas.

**No global ordering** between Event and Delta lanes. `MessageEnded` and `MessageCommitted` may arrive in either order.

## SessionOutputHub

Session-scoped fan-out — not turn-scoped. Root and all child tasks for a session share one hub keyed by `session_id`.

```text
TaskRuntime
  → TaskEventEmitter
      ├─ PersistSink              (durable facts → hostd)
      ├─ InternalLifecycleObserver (supervisor registry)
      └─ SessionOutputHub
          ├─ reliable event lane  → SessionSubscription
          └─ realtime delta lane  → SessionSubscription
```

Observation rules:

- hostd durable state updates at `PersistSink` commit time — not when `SessionEvent` is received.
- Supervisor registry updates via internal lifecycle observer — not via session output feedback.
- `SessionEvent` is a **notification** after state changed; it is not a state-machine input.
- `RealtimeDelta` drives live rendering only.

## Subscription model

- Scope: **session_id** (all tasks in the session, including dynamically spawned children).
- Optional filter: `SubscribeRequest.task_id`.
- Commands (`create_task`, `submit_input`) do **not** return per-task streams.

Recommended client flow:

```text
session_snapshot → record cursor → subscribe_session(after = cursor)
  → apply reliable SessionEvent
  → render RealtimeDelta opportunistically
```

On `SnapshotRequired`: fetch a fresh snapshot, resubscribe from its cursor.

Subscription disconnect must not terminate tasks. Task idle/closed must not close the session subscription.

## Lifecycle projection

Task/work lifecycle is committed as a durable fact, observed internally, then optionally projected as:

- `SessionEvent::TaskChanged`
- `SessionEvent::WorkChanged`

There is no public lifecycle stream. hostd must not reconstruct authoritative state from lifecycle notifications alone.

## Command acknowledgement

Separate from observation and transcript:

```text
TaskHandle        ← create_task
InputReceipt      ← submit_input
TaskSnapshot      ← control_task
```

These do not enter the transcript.

## Testing

Tests use `CollectingPersistSink` and hub subscriptions via `orchd::testing` helpers. Production hub and test collectors must emit the same business event semantics.

See [testing.md](testing.md).

## Related reading

- [public-api.md](public-api.md) — `subscribe_session` API
- [host-integration.md](host-integration.md) — how hostd consumes output
- [persistence.md](persistence.md) — when events become durable
