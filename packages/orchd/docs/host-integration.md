# Host Integration

> Status: current  
> Audience: hostd integrators

orchd is linked into hostd as an **in-process Rust library**. There is no RPC. Production turns use `AgentRuntime`; bootstrap uses `orchd::host`.

Identity conventions: [`docs/agent-identity.md`](../../../docs/agent-identity.md)

## Crate surface for hostd

| Module | When to use |
|---|---|
| `orchd::api` | All turn commands and observation |
| `orchd::host` | Process startup, tool registration, approval, persist sink injection |
| `orchd::integration` | `TaskRepository` implements `PersistSink` |

Wire types (`OrchdConfig`, `AgentSpec`, `SubmitTaskInput`, …) come from `piko-protocol`.

## Bootstrap

Once per process (or per hostd instance):

```rust
let supervisor = Supervisor::from_config(model_executor, OrchdConfig {
    providers,
    agents,
    default_model,
    default_settings,
    runtime: Default::default(),
    thinking_level_map,
    sandbox,
}).await;

// MCP tools, approval gateway, user-interaction provider …
supervisor.set_persist_sink(
    Arc::new(task_repository) as Arc<dyn PersistSink>
).await;

let runtime = AgentRuntimeService::new(Arc::clone(&supervisor));
```

`Supervisor::from_config` registers built-in tool providers (workspace, task_control, todo) and wires the internal `TaskControlPort` (for agent spawn/steer tools). **hostd does not call `TaskControlPort`.**

## End-to-end turn flow

```mermaid
sequenceDiagram
    participant TUI
    participant Host as hostd
    participant RT as AgentRuntimeService
    participant Hub as SessionOutputHub
    participant Store as TaskRepository

    TUI->>Host: TurnSubmit
    Host->>RT: start_root_turn (subscribe + create/reuse + submit_input)
    RT-->>Host: SessionSubscription
    Note over Host,Store: user message via submit_input only
    Host->>Host: consume SessionOutput stream
    Host->>Host: project MessageCommitted → HostState
    Host->>TUI: Display / TaskLifecycle events
```

hostd **never** appends user messages directly to JSONL on the TurnSubmit path. Every user message goes through `submit_input` → `PersistSink::commit_message`.

## API mapping

### Root TurnSubmit

```text
TUI TurnSubmit
  → hostd expands templates / system prompt
  → start_root_turn(session, turn_id, work_id, "main", prompt, resume?)
      ├─ subscribe_session
      ├─ create_task(main) or reuse idle root
      └─ submit_input(root, prompt)
  → drain SessionSubscription until root is idle/terminal
  → project Event/Delta to TUI
```

```rust
let subscription = runtime.start_root_turn(
    &session_id,
    &turn_id,        // source_turn_id
    &work_id,
    "main",
    &prompt,
    resume_state,    // TaskResumeState from task shard, or None
    resume_task_id,
).await?;
```

hostd still calls `supervisor.register_agent(root_spec)` each turn to inject the system prompt; this may move to session initialization later.

### Subsequent input

```rust
runtime.submit_input(build_user_input(
    &session_id,
    &task_id,
    &work_id,
    content,
    InputSource::User,
    Some(turn_id),
)).await?;
```

### Queue steer

Steer is not a separate control channel — it is `submit_input`:

```rust
runtime.submit_input(build_user_input(
    &session_id,
    &task_id,
    &work_id,
    MessageContent::String(message),
    InputSource::Task {
        task_id: source_task_id.into(),
        agent_id: source_agent_id.into(),
    },
    None,
)).await?;
```

The hostd queue only decides **when** to call `submit_input`, not how transcript mutation or persistence works.

### Spawn (agent tools, not hostd)

```text
parent spawn tool
  → create_task(child, parent_task_id)
  → submit_input(child, initial prompt)
  → optionally await work report
```

`spawn` vs `spawn_detached` differs only in whether the parent waits for the result; child initialization is identical. Handled internally by `TaskControlPort`; hostd observes child events on the same `SessionSubscription`.

### Task control

```rust
runtime.control_task(TaskControlRequest::CancelWork { request_id, task_id, work_id }).await?;
runtime.control_task(TaskControlRequest::Close { request_id, task_id }).await?;
runtime.control_task(TaskControlRequest::Terminate { request_id, task_id }).await?;
```

## Consuming SessionOutput

| Output | hostd action |
|---|---|
| `SessionOutput::Delta` | Project to `DisplayEvent` for TUI streaming |
| `SessionOutput::Event::TaskChanged` | Project to `TaskLifecycle`; update agent panel |
| `SessionOutput::Event::MessageCommitted` | Read committed message from `TaskRepository`; project to `HostState` (no JSONL write) |
| `SessionOutput::Event::ToolCommitted` | Same as above |

When `MessageCommitted` arrives, the durable write is already complete; hostd only projects into memory and updates manifest metadata.

Recommended reconnect flow (not yet fully implemented in hostd):

```text
session_snapshot → record cursor → subscribe_session(after = cursor)
```

## PersistSink implementation

hostd `TaskRepository` implements `orchd::integration::PersistSink`:

- Per-task shard: `tasks/{task_id}.jsonl`
- Session manifest: `session.json`
- Per-task head and `task_seq` ordering

orchd awaits `PersistAck` at user input commit before entering an LLM step. Details: [persistence.md](persistence.md) (PR 2).

## Child tasks

Child tasks share the parent's session-scoped hub. hostd does not need a separate subscription per child; route by `task_id` / `agent_id` on the envelope.

## Known gaps

| Area | Status |
|---|---|
| `TurnCancel` | hostd updates in-memory Turn state only; not wired to `control_task(CancelWork)` |
| Session reconnect | snapshot + cursor resubscribe not implemented |
| `jsonl_repository::append_entry(Message)` | legacy direct-write path; TurnSubmit does not use it |
| `Supervisor` exposed directly | transitional; target is a narrow `HostRuntime` bootstrap type |

## Related reading

- [public-api.md](public-api.md) — full API contract
- [overview.md](overview.md) — architecture boundaries and design decisions
