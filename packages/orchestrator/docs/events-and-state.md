# Events And State

`OrchestratorEvent` is the public fact stream. `StateActor` owns ingestion,
event log, reducer projection, subscriptions, snapshots, and graph projection.

## Event Model

```mermaid
flowchart LR
  Message[Actor message<br/>command or request]
  Work[Business actor work]
  Event[OrchestratorEvent<br/>fact after transition]
  StateActor[StateActor<br/>event log owner]
  Reducer[reduce()<br/>pure projection]
  State[OrchestratorState]

  Message --> Work --> Event --> StateActor --> Reducer --> State
```

Business actors publish events with an injected async emitter:

```ts
interface ActorDeps {
  emit(event: OrchestratorEvent, options?: EmitOptions): Promise<void>;
}
```

The emitter is implemented as `ask("orchestrator:state", ingest_event)`:

```ts
function createEmitter(actorSystem: ActorSystem): ActorDeps["emit"] {
  return async (event) => {
    await actorSystem.ask("orchestrator:state", {
      type: "ingest_event",
      event,
    });
  };
}
```

`await emit(event)` is a deliberate pause/resume point. The business actor
yields while `StateActor` serializes the event, reduces it into state, notifies
subscribers, and replies.

Consistency rule:

```mermaid
sequenceDiagram
  participant Actor as BusinessActor
  participant State as StateActor
  participant Host as Host

  Actor->>State: await emit(event)
  State->>State: append event log
  State->>State: reduce into OrchestratorState
  State-->>Actor: emit resolved
  Host->>State: await snapshot()
  State-->>Host: state includes resolved event
```

## StateActor

Messages:

```ts
type StateMsg =
  | { type: "ingest_event"; event: OrchestratorEvent }
  | { type: "snapshot" }
  | { type: "dump_events" }
  | { type: "render_graph" }
  | { type: "subscribe"; listener: OrchestratorEventListener }
  | { type: "unsubscribe"; subscriptionId: string };
```

Behavior:

```ts
async function stateActor(msg: StateMsg, ctx: ActorContext, meta: Envelope) {
  if (msg.type === "ingest_event") {
    const envelope = createEventEnvelope(msg.event);
    eventLog.push(envelope);
    state = reduce(state, envelope);
    for (const listener of listeners) listener(envelope, state);
    ctx.reply(meta, envelope);
    return;
  }

  if (msg.type === "snapshot") {
    ctx.reply(meta, structuredClone(state));
    return;
  }
}
```

Reducer responsibilities:

- deterministic and side-effect free
- no `await`
- no actor messaging
- no scheduling decisions

The actor owns event ingestion. The pure reducer only folds one event envelope
into one state value.

## Event Envelope

```ts
export interface OrchestratorEventEnvelope {
  id: string;
  runId: string;
  seq: number;
  time: number;
  event: OrchestratorEvent;
}
```

No public state mutation may bypass `StateActor ingest_event -> reduce`.

## Core Events

```ts
type OrchestratorEvent =
  | { type: "orchestrator_started" }
  | { type: "orchestrator_stopped"; reason?: string }
  | { type: "actor_spawned"; actorId: string; kind: string }
  | { type: "actor_stopped"; actorId: string; reason?: string }
  | { type: "actor_error"; actorId: string; message: string }
  | { type: "agent_registered"; agent: AgentSpec }
  | { type: "agent_unregistered"; agentId: string }
  | { type: "task_created"; task: AgentTask }
  | { type: "task_started"; agentId: string; taskId: string }
  | { type: "task_delta"; agentId: string; taskId: string; delta: AgentDelta }
  | { type: "task_completed"; agentId: string; taskId: string; result: AgentTaskResult }
  | { type: "task_failed"; agentId: string; taskId: string; error: string }
  | { type: "task_cancelled"; agentId: string; taskId: string; reason?: string }
  | { type: "plan_updated"; agentId: string; taskId: string; plan: AgentPlan }
  | { type: "tool_started"; agentId: string; taskId: string; callId: string; name: string }
  | { type: "tool_finished"; agentId: string; taskId: string; callId: string; result: unknown }
  | { type: "approval_requested"; approval: ApprovalRequest }
  | { type: "approval_resolved"; approvalId: string; decision: ApprovalDecision };
```

## Snapshot And Graph

`snapshot()` asks `StateActor` for the reducer projection:

```ts
interface OrchestratorState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  agents: Record<string, AgentRuntimeState>;
  tasks: Record<string, AgentTaskState>;
  approvals?: Record<string, ApprovalRequestState>;
}
```

`approvals` is a projection of approval events emitted around HostProvider
promises. It does not imply a dedicated ApprovalActor.

Plans are task state, not global state:

```ts
interface AgentTaskState {
  id: string;
  agentId: string;
  status: AgentTaskStatus;
  plan?: AgentPlan;
}
```

Each agent task may have its own plan. A coordinator's plan, an implementer's
plan, and a reviewer's plan should not overwrite each other.

`renderGraph()` asks `StateActor` for a graph derived from state and recent
events:

- agent nodes
- task nodes
- approval nodes
- edges for running task, waiting approval, delegation

Graph rendering is a projection only. It must not affect scheduling.

## Event Usage By Actor

| Actor | Emits |
| --- | --- |
| `MainActor` | `task_created`, optional top-level run lifecycle |
| `AgentActor` | `task_started`, `task_delta`, `plan_updated`, `task_completed`, `task_failed`, `task_cancelled` |
| `ToolActor` / providers | `tool_started`, `tool_finished`, approval events |
| `TimerActor` / `WatchActor` | watch registered/fired/cancelled events |

Constraints:

- Emit only after the actor's private state transition has happened.
- Emit terminal task events exactly once.
- Do not use `OrchestratorEvent` for actor-to-actor communication.
- Await `emit()` for task/tool/approval lifecycle events.
- Events should be serializable and stable enough for Host/TUI/RPC/debugging.
