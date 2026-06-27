# orchd — Runtime events, observability & testability

## Current status

| Area | Status | Notes |
|---|---|---|
| Runtime events | implemented | orchd emits `piko_protocol::Event` notifications to hostd listeners. |
| Runtime projection | implemented | `OrchCore::snapshot()` reads in-memory agents, tool sets, and task states. |
| Durable session log | owned by hostd | hostd persists `SessionTreeEntry` JSONL records and is the only recovery source. |
| Testability | solid | Tests cover orchestration, tools, events, error paths, and concurrency with `FauxProvider`. |

## Boundary

orchd must not maintain a second persistent event-sourcing model. Its events are
runtime notifications used by hostd and host-tui while a process is alive.

```
orchd runtime activity
        │
        ▼
piko_protocol::Event
        │
        ▼
hostd
        │
        ├── updates live UI state
        └── appends durable SessionTreeEntry JSONL records when the event is a session fact
```

Restart and resume semantics come from hostd's session storage. Any new durable
conversation fact should be represented as a `SessionTreeEntry`, not as an
orchd-local journal event.

## Runtime projection

`OrchCore` keeps ephemeral state needed for inspection and coordination:

```rust
agent_specs: RwLock<HashMap<String, AgentSpec>>
task_states: RwLock<HashMap<String, AgentTaskState>>
tool_registry: ToolRegistryImpl
```

`snapshot()` reports that projection. It is not replayable and should not be
used as the source of truth for session recovery.
