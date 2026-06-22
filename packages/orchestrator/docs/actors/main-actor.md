# Orchestrator Facade

## Design

The `Orchestrator` class is the DI root and public API facade. It:

- Stores `agentSpecs: Map<string, AgentSpec>` — registered agent specs
- Stores `runs: Map<string, RunHandle>` — active/recent task run handles
- Stores `allocatedTaskIds: Set<string>` — permanent record of all ever-used task IDs (prevents reuse after eviction)
- Holds a reference to `InMemoryEventStore`, `ToolRegistryImpl`, `ModelStepExecutor`, and `ActorSystem`

Public API calls are **direct method calls** on the Orchestrator object, **not** actor messages:

```text
orchestrator.run(prompt)           → task.run(ctx, prompt, opts)
orchestrator.dispatch(task)        → task.dispatch(ctx, task)
orchestrator.cancelTask(taskId)    → task.cancelTask(ctx, taskId)
orchestrator.registerAgent(spec)   → agent.registerAgent(ctx, spec)
orchestrator.snapshot()            → ctx.eventStore.snapshot()
orchestrator.subscribe(listener)   → ctx.eventStore.subscribe(listener)
```

## Task Dispatch Flow

```mermaid
sequenceDiagram
  participant Host
  participant Facade as Orchestrator facade
  participant Store as InMemoryEventStore
  participant Kernel as ActorSystem
  participant Agent as AgentActor (task-scoped)

  Host->>Facade: run(prompt, options)
  Facade->>Store: append(orchestrator_started)
  Facade->>Facade: createRun() — allocate taskId, validate agent
  Facade->>Store: append(task_created)
  Facade->>Kernel: spawn(agent:<agentId>:task:<taskId>)
  Facade->>Kernel: ask(dispatch task)
  Agent-->>Facade: AgentTaskResult (via pendingReply)
  Facade-->>Host: OrchRunResult
```

## Run Handle

`createRun()` returns a `RunHandle` which tracks the task's status and
wraps `resultPromise`. The handle is stored in `ctx.runs`:

```ts
interface RunHandle {
  taskId: string;
  agentId: string;
  actorId: string;   // "agent:<agentId>:task:<taskId>"
  status: "starting" | "running" | "cancelling" | "completed" | "failed" | "cancelled";
  retainForJoin: boolean;
  resultPromise: Promise<any>;
}
```

Status transitions:
- `"starting"` — set immediately on creation, before agent lookup
- `"running"` — set after agent is found and actor spawned
- `"cancelling"` — set on cancelTask before sending cancel message to actor
- `"completed"`, `"failed"`, `"cancelled"` — set when resultPromise resolves

When there are 100 or more settled runs, `createRun()` evicts settled
non-detached (`retainForJoin: false`) entries to prevent memory growth.
Detached runs have `retainForJoin: true` and remain addressable for repeated
`joinTask()` calls for the lifetime of the Orchestrator. Task IDs are still
tracked in `allocatedTaskIds` after non-detached eviction so they cannot be
reused.

## Cancellation Flow

```mermaid
sequenceDiagram
  participant Host
  participant Facade as Orchestrator facade
  participant Kernel as ActorSystem
  participant Agent as AgentActor (task-scoped)

  Host->>Facade: cancelTask(taskId)
  Facade->>Facade: look up runs[taskId]
  alt task already settled (completed/failed/cancelled)
    Facade-->>Host: return (no-op)
  else task is active
    Facade->>Facade: run.status = "cancelling"
    Facade->>Kernel: ask(agent:..., cancel msg)
    Agent->>Agent: abort signal + status = "cancelling"
    Agent-->>Facade: ok (cancel acknowledged)
    Note over Agent: Worker detects abort, sends runner_finished/failed
    Agent->>Facade: run.resultPromise resolves
  end
```

## Agent Registration

`registerAgent(spec)` and `unregisterAgent(agentId)` are synchronous mutations
on `ctx.agentSpecs`. They emit `agent_registered` / `agent_unregistered` events.
No actor is spawned for the agent itself — actors are spawned per task.

## Task ID Uniqueness

- `allocatedTaskIds.has(taskId)` is checked before any task is created
- If the ID has ever been used, `createRun()` throws `"Duplicate task ID: <id>"`
- This guarantee holds even after `runs` eviction

## Invariants

- `task_created` is emitted in `createRun()` before the AgentActor is spawned.
- `task_started` is emitted by the AgentActor at the beginning of `handleDispatch`.
- `task_created` ordering is enforced by `allocatedTaskIds` before insertion.
- `cancelTask` only transitions to `"cancelling"` if the run is active; it returns early (no-op) for settled tasks (`completed`, `failed`, `cancelled`).
- `cancelTask` rolls back the `"cancelling"` transition if the actor ask fails (e.g. `ActorNotFoundError`).
- Duplicate task IDs are rejected at `createRun()` time via `allocatedTaskIds.has()`. This guarantee holds even after run handle eviction.
- Every public task operation returns a clear not-found error for unknown task IDs.
- `AgentActor` guards against emitting more than one terminal event via `terminalCommitted`.
