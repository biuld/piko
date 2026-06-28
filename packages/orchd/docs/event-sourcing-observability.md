# orchd — Runtime events, observability & testability

## Event architecture

orchd produces a single `Stream<Item = piko_protocol::Event>` per run. No pub/sub,
no listener registry, no separate event sink abstraction.

```
agent loop (stream! macro)
        │
        │  yield Event::TaskStarted
        │  yield Event::TextDelta
        │  yield Event::ToolStart
        │  ...
        ▼
Pin<Box<dyn Stream<Item = Event>>>
        │
        │  hostd reads via stream.next().await
        ▼
hostd → TUI (JSONL)
```

## Event types

All events are `piko_protocol::Event` enum variants. The protocol crate is the
single source of truth for the event vocabulary.

### Per-step events (emitted by agent loop)

| Event | When |
|---|---|
| `MessageStart` | LLM call begins |
| `TextDelta` | Each token chunk from LLM |
| `ThinkingDelta` | Each reasoning chunk (extended thinking) |
| `MessageEnd` | LLM call ends |
| `ToolStart` | Tool execution begins |
| `ToolEnd` | Tool execution ends (result or error) |
| `ApprovalRequested` | Tool requires user approval |
| `ApprovalResolved` | User approved/declined |

### Task lifecycle events

| Event | When |
|---|---|
| `TaskCreated` | Task is queued for an agent |
| `TaskStarted` | Agent begins processing task |
| `TaskCompleted` | Task finished successfully |
| `TaskFailed` | Task terminated with error |
| `TaskCancelled` | Task was cancelled |
| `TaskSteered` | Follow-up message injected mid-task |
| `TaskJoined` | Detached sub-task completed and result available |

### Turn events

| Event | When |
|---|---|
| `TurnStarted` | New user turn begins |
| `TurnCompleted` | Turn finished (all tasks done) |
| `TurnFailed` | Turn terminated with error |

## Boundary

orchd does not maintain durable event storage. Events are runtime notifications
consumed by hostd. Hostd is responsible for:

- Forwarding events to the TUI as JSONL
- Persisting session facts as `SessionTreeEntry` records
- Recovery from session JSONL on restart

## Runtime projection

`OrchCore` keeps ephemeral state for inspection:

```rust
agent_specs: RwLock<HashMap<String, AgentSpec>>
task_states: RwLock<HashMap<String, AgentTaskState>>
tool_registry: Arc<ToolRegistryImpl>
running_tasks: Mutex<HashMap<String, RunningTaskControl>>
```

`snapshot()` reports that projection. It is diagnostic, not a recovery log.

## Testability

Tests use `FauxProvider` to mock the LLM layer. The `run_streaming()` API returns
a `Pin<Box<dyn Stream>>` that tests can consume directly:

```rust
let mut stream = core.run_streaming("hello", opts).await;
let mut events = Vec::new();
while let Some(event) = stream.next().await {
    events.push(event);
}
// assert on events
```

### Test coverage

- Unit tests: `ToolRegistryImpl` policy projection
- Integration tests: orchestration, events, error paths, concurrency
- Faux provider: canned LLM responses and tool calls

## Tracing

Agent execution uses `#[tracing::instrument]` for structured observability:

```
run_agent_task { task_id, agent_id }
  run_agent_loop { task_id }
    run_model_step { task_id, step }
    execute_tool_calls { task_id, tool_count }
```

The `stream!` macro blocks are not directly instrumented, but the async operations
inside them (model calls, tool execution) carry tracing spans.
