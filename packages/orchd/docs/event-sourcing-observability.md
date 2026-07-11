# orchd — Runtime events and observability

> **Partially superseded.** Reliable production output now uses `SessionOutput` envelopes
> (`SessionOutput::Event` / `SessionOutput::Delta`) published through `SessionOutputHub`.
> See [`docs/agent-runtime-api-design.md`](../../../docs/agent-runtime-api-design.md) §4.6
> and [`host-interface.md`](host-interface.md).

This document retains notes on event vocabulary and testability patterns. Where it
conflicts with the Agent Runtime API design, prefer the design doc.

## Observation model (current)

```
task runtime
    │
    ├─ PersistSink::commit_*  (durable barrier)
    │
    ├─ SessionOutputHub::publish_event   → SessionOutput::Event
    └─ SessionOutputHub::publish_delta   → SessionOutput::Delta
            │
            ▼
    hostd SessionSubscription stream
            │
            ▼
    TUI projection (DisplayEvent, TaskLifecycle, …)
```

hostd subscribes once per session. Child tasks do not get separate output channels.

## Reliable session events

Post-commit notifications (after `PersistSink` ack):

| Event | When |
|---|---|
| `TaskChanged` | Task lifecycle transition (`TaskSnapshot`) |
| `MessageCommitted` | User/assistant/tool message durable in task shard |
| `ToolCommitted` | Tool result message durable in task shard |
| `WorkChanged` | Work cycle status update |

## Realtime deltas

Best-effort streaming UI updates (`RealtimeDelta`):

| Delta | When |
|---|---|
| `MessageStarted` | LLM response begins |
| `Text` / `Thinking` / `ToolCall` | Token chunks |
| `MessageEnded` | LLM response ends |

Deltas may be dropped under subscriber lag. `MessageCommitted` is authoritative.

## Legacy `piko_protocol::Event` stream

Earlier orchd versions exposed a single `Stream<Item = Event>` per run with typed
lifecycle variants (`TaskCreated`, `TextDelta`, …). That path is replaced by the
session hub model above. Tests may still adapt hub output into legacy event shapes
for assertions.

## Testability

- **`CollectingPersistSink`** — in-memory `PersistSink` for unit/integration tests
- **`SessionOutputHub`** — subscribe in tests to assert reliable events and deltas
- **`AgentRuntimeService::start_root_turn`** — end-to-end turn bootstrap helper
- **`Supervisor::run`** — synchronous drain helper for tooling (not for hostd production)

## Tracing

Task and step execution emit `tracing` spans keyed by `session_id`, `task_id`,
`work_id`, and `message_id`. Correlate hub envelopes with persist commits via
shared identifiers in structured logs.
