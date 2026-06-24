# orchd — Event sourcing, observability & testability

## Current state

| Capability | Status | Notes |
|---|---|---|
| Event sourcing | ✅ implemented | `OrchSourcingEvent` enum, `apply_event()`, `rebuild_state()`. Journal stored directly in `OrchCore.sourcing_events` (no trait abstraction). |
| Observability | ✅ wired | `#[tracing::instrument]` on all key paths. |
| Testability | ✅ solid | 47 tests: unit + integration + event sourcing + error paths + concurrency. `FauxProvider` supports error simulation. |

## Architecture

```
Command ──► handle ──► OrchSourcingEvent ──► sourcing_events.push()
                                 │
                                 └──► apply_event() ──► OrchState (replay)
```

State changes produce sourcing events stored in an append-only `Vec<OrchSourcingEvent>`.
Replay is a pure fold: `rebuild_state(&events)`.

Three layers of observability:

```
Layer 1: tracing spans      — performance / latency / call chains
Layer 2: Sourcing events    — state change journal (replayable, auditable)
Layer 3: OrchEvent          — real-time push to Host (TUI rendering)
```

## Event sourcing

### OrchSourcingEvent

| Event | Emitted when |
|---|---|
| `AgentRegistered` | `OrchCore::register_agent()` |
| `AgentUnregistered` | `OrchCore::unregister_agent()` |
| `TaskCreated` | `OrchCore::spawn()` / `spawn_detached()` |
| `TaskStarted` | Agent loop begins |
| `TaskStepCompleted` | Each model step finishes |
| `TaskToolCalled` | Tool execution begins |
| `TaskToolResult` | Tool execution completes |
| `TaskCompleted` | Agent loop finishes successfully |
| `TaskFailed` | Agent loop fails |
| `TaskCancelled` | `OrchCore::cancel_task()` |
| `ModelConfigSet` | `OrchCore::set_model_config()` |
| `ToolSetRegistered` | `OrchCore::register_tool_set()` |
| `ToolSetUnregistered` | `OrchCore::unregister_tool_set()` |

### OrchCore journal

```rust
pub struct OrchCore {
    sourcing_events: RwLock<Vec<OrchSourcingEvent>>,
}

impl OrchCore {
    // Appended automatically by every state-changing method.
    async fn emit_sourcing(&self, event: OrchSourcingEvent) {
        self.sourcing_events.write().await.push(event);
    }

    // Read journal (for tests/debugging).
    pub async fn sourcing_events(&self) -> Vec<OrchSourcingEvent> { ... }
}
```

### State reconstruction (pure functions)

```rust
/// Apply one event to state.
pub fn apply_event(state: OrchState, event: &OrchSourcingEvent) -> OrchState { ... }

/// Rebuild state from a slice of events.
pub fn rebuild_state(events: &[OrchSourcingEvent]) -> OrchState { ... }
```

## Observability

`#[tracing::instrument]` on all key execution paths:

| Function | File | Fields |
|---|---|---|
| `start_agent_run` | `runner.rs` | `task_id`, `agent_id` |
| `run_engine_loop` | `engine_loop.rs` | `task_id` |
| `run_model_step` | `step_runner.rs` | `task_id` |
| `execute_tool_calls` | `tool_executor.rs` | `task_id`, `tool_count` |

Span tree: `start_agent_run → run_engine_loop → [run_model_step, execute_tool_calls]`.

## Testability

| Category | Tests | Location |
|---|---|---|
| Event sourcing (unit) | `apply_event`, `rebuild_state`, event kinds | `src/protocol/event_store.rs` `#[cfg(test)]` |
| Config/types (unit) | OrchdError, TaskInput, OrchdConfig, serde | `src/protocol/config.rs` `#[cfg(test)]` |
| Tool registry (unit) | Approval policy | `src/tools/registry.rs` `#[cfg(test)]` |
| Event sourcing (integration) | Core emission, journal reads | `tests/event_sourcing.rs` |
| Integration extended | Tool sets, cancel, empty response, concurrency, subscribe | `tests/integration_extended.rs` |
| Orchestrator (integration) | Agent lifecycle, spawn, run | `tests/orchestrator_integration.rs` |
| Kernel (integration) | tokio-actors | `tests/kernel_integration.rs` |

47 tests total. `FauxProvider` supports `push_text()` and `push_error()` for mocking LLM responses and failures.

## Event emission checklist

| Entry point | Sourcing events |
|---|---|
| `OrchCore::register_agent` | `AgentRegistered` |
| `OrchCore::unregister_agent` | `AgentUnregistered` |
| `OrchCore::spawn` | `TaskCreated`, `TaskStarted` |
| `OrchCore::spawn_detached` | `TaskCreated` |
| `OrchCore::cancel_task` | `TaskCancelled` |
| `OrchCore::set_model_config` | `ModelConfigSet` |
| `OrchCore::register_tool_set` | `ToolSetRegistered` |
| `OrchCore::unregister_tool_set` | `ToolSetUnregistered` |
| Agent loop (internal) | `TaskStepCompleted`, `TaskToolCalled`, `TaskToolResult`, `TaskCompleted` / `TaskFailed` |
