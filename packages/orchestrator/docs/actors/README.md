# Actors

> [!NOTE]
> The Orchestrator uses a **task-scoped actor model**. AgentActors are spawned
> per task, not per registered agent. There is no persistent `orchestrator:main`
> or `orchestrator:state` actor — those responsibilities are now handled by the
> `Orchestrator` facade and `InMemoryEventStore` directly.

## Current Actor

| Actor | ID | Owns |
| --- | --- | --- |
| [AgentActor](agent-actor.md) | `agent:<agentId>:task:<taskId>` | agent transcript, model step loop, task state, tool execution |

Each AgentActor is spawned when a task is dispatched and stops itself after
emitting its terminal event (completed / failed / cancelled). Cross-actor
communication goes through `send()` / `ask()` / `reply()` only.

## Former Actors (now replaced)

| Former Actor | ID | Replaced By |
| --- | --- | --- |
| `MainActor` | `orchestrator:main` | `Orchestrator` facade methods (`task.ts`) |
| `StateActor` | `orchestrator:state` | `InMemoryEventStore` (synchronous) |
| `ToolActor` | `tool:<agentId>:step_<n>` | `ToolRegistryImpl.executeTool()` (stateless method call) |

## State and Event Flow

`AgentActor` receives `emit` via `AgentActorDeps`. Emit is a direct synchronous
call to `InMemoryEventStore.append()`, wired in the Orchestrator constructor.

Model-step deltas are mapped to Orchestrator events:

| ModelStepEvent | Emitted Event |
| --- | --- |
| `message_start` | `task_message_start` |
| `message_delta` | `task_delta` |
| `thinking_delta` | `task_delta` (kind: "thinking") |
| `message_update` | `task_message_update` |
| `message_end` | `task_message_end` |

Approval/user interaction is handled by `HostToolProvider` via the
`ApprovalGateway`. See [events-and-state.md](../events-and-state.md) for details.
