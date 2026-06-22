# Events And State

`OrchestratorEvent` is the public fact stream. `InMemoryEventStore` owns event
ingestion, event log, reducer projection, subscriptions, snapshots, and graph
projection. It is a plain synchronous class — not an actor.

## Event Model

```mermaid
flowchart LR
  Message[AgentActor message]
  Work[Agent work]
  Event[OrchestratorEvent<br/>fact after transition]
  Store[InMemoryEventStore<br/>event log owner]
  Reducer[reduceStateEvent()<br/>pure projection]
  HostEventConv[eventToHostEvent()]
  State[StateActorState]

  Message --> Work --> Event --> Store --> Reducer --> State
  Store --> HostEventConv --> Subscribers
```

Business actors publish events with an injected async emitter:

```ts
interface AgentActorDeps {
  emit(event: OrchestratorEvent): Promise<void>;
}
```

The emitter is wired directly to `eventStore.append()` in the Orchestrator
constructor:

```ts
const emit = async (event: OrchestratorEvent) => {
  this.eventStore.append(event);
};
```

`append()` is **synchronous** — it reduces the event into state and notifies
subscribers before returning. `await emit(event)` resolves in the same
microtask tick.

Consistency rule:

```text
append(event) is synchronous — state is updated before the call returns
await emit(event) always observes the updated state on the next line
snapshot() after await emit(event) always includes that event
```

## InMemoryEventStore

```ts
class InMemoryEventStore implements EventStore {
  append(event: OrchestratorEvent): OrchestratorEventEnvelope;
  subscribe(listener: HostEventListener): () => void;
  snapshot(): OrchState;
  graph(): { nodes: ...; edges: ... };
  dumpEvents(): OrchestratorEventEnvelope[];
}
```

`append()` sequence:

```text
1. seq++
2. build OrchestratorEventEnvelope { id, runId, seq, time, event }
3. push to eventLog
4. reduceStateEvent(state, envelope) — synchronous in-place reducer
5. map event to HostEvent via eventToHostEvent(event, envelope, state)
6. notify all subscribers synchronously (skip if HostEvent is null)
7. return envelope
```

Subscriber errors are swallowed so they cannot disrupt state updates.

## subscribe()

`subscribe(listener)` returns an unsubscribe function:

```ts
const unsub = orchestrator.subscribe((hostEvent) => { ... });
// later:
unsub();
```

Listeners receive `HostEvent` objects (the public projection of internal
`OrchestratorEvent`s via `eventToHostEvent()`), not raw envelopes.

Subscriber management lives on `InMemoryEventStore` directly (not stored inside
`StateActorState`).

## Event Envelope

```ts
export interface OrchestratorEventEnvelope {
  id: string;    // "evt_<seq>"
  runId: string;
  seq: number;   // monotonically increasing
  time: number;  // Date.now() at time of append
  event: OrchestratorEvent;
}
```

## Core Events

```ts
type OrchestratorEvent =
  // Lifecycle
  | { type: "orchestrator_started" }
  | { type: "orchestrator_stopped"; reason?: string }
  // Actor kernel events
  | { type: "actor_spawned"; actorId: string; kind: string }
  | { type: "actor_stopped"; actorId: string; reason?: string }
  | { type: "actor_error"; actorId: string; message: string }
  // Agent management
  | { type: "agent_registered"; agent: AgentSpec }
  | { type: "agent_unregistered"; agentId: string }
  // ToolSet management
  | { type: "tool_set_registered"; toolSet: ToolSet }
  | { type: "tool_set_unregistered"; toolSetId: string }
  // Task lifecycle
  | { type: "task_created"; task: AgentTask }
  | { type: "task_started"; agentId: string; taskId: string }
  | { type: "task_delta"; agentId: string; taskId: string; delta: unknown }
  | { type: "task_message_start"; agentId: string; taskId: string; message: RuntimeMessage }
  | { type: "task_message_update"; agentId: string; taskId: string; message: RuntimeMessage; assistantEvent?: RuntimeAssistantMessageEvent }
  | { type: "task_message_end"; agentId: string; taskId: string; message: RuntimeMessage }
  | { type: "task_completed"; agentId: string; taskId: string; result: AgentTaskResult }
  | { type: "task_failed"; agentId: string; taskId: string; error: string }
  | { type: "task_cancelled"; agentId: string; taskId: string; reason?: string }
  | { type: "task_transcript_committed"; agentId: string; taskId: string; messages: Message[]; summary: string; finalStatus: string }
  | { type: "plan_updated"; agentId: string; taskId: string; plan: unknown }
  // Tool execution
  | { type: "tool_started"; agentId: string; taskId: string; callId: string; name: string; args: Record<string, unknown> }
  | { type: "tool_finished"; agentId: string; taskId: string; callId: string; result: unknown }
  // Approval
  | { type: "approval_requested"; approval: unknown }
  | { type: "approval_resolved"; approvalId: string; decision: string };
```

## Snapshot And Graph

`snapshot()` returns a deep clone of the current reducer projection:

```ts
interface OrchState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  toolSets: Record<string, ToolSet>;
  agents: Record<string, AgentRuntimeState>;
  tasks: Record<string, AgentTaskState>;
}
```

`graph()` returns nodes and edges derived from the current state:

- agent nodes (id: `agent:<id>`)
- task nodes (id: `task:<id>`)
- edges: agent → task (`"owns"` when agent has active task)
- edges: task → parent task (`"parent"` when task has parentTaskId)

Graph rendering is a pure projection. It does not affect scheduling.

Plans are task state, stored via the `plan_updated` event. The reducer stores
`plan` on the task's `result` field:

```ts
// In reduceStateEvent, plan_updated → task.result = { ...task.result, plan: event.plan }
```

## StateActorState (internal)

```ts
interface StateActorState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  eventLog: OrchestratorEventEnvelope[];
  seq: number;
  agents: Record<string, AgentRuntimeState>;
  tasks: Record<string, AgentTaskState>;
  toolSets: Record<string, ToolSet>;
  locks: Record<string, unknown>;
  /** Tool call metadata for HostEvent mapping. */
  callMetas: Map<string, CallMeta>;
}
```

Note: Subscriber management lives on `InMemoryEventStore` directly.
The `listeners` and `nextSubId` fields on `StateActorState` are unused at runtime.

## Event Usage By Component

| Component | Emits |
| --- | --- |
| `Orchestrator` facade | `orchestrator_started`, `task_created`, `agent_registered`, `agent_unregistered`, `plan_updated` |
| `orchestrator/agent.ts` | `agent_registered`, `agent_unregistered` |
| `orchestrator/task.ts` | `task_created`, `task_failed` (on agent not found or actor error) |
| `orchestrator/tool.ts` | `tool_set_registered`, `tool_set_unregistered` |
| `AgentActor` | `task_started`, `task_delta`, `task_message_start`, `task_message_update`, `task_message_end`, `task_transcript_committed`, `task_completed`, `task_failed`, `task_cancelled` |
| `ToolRegistryImpl.executeTool()` | `tool_started`, `tool_finished`, `approval_resolved` |

Constraints:

- Emit only after the component's private state transition has happened.
- Emit terminal task events exactly once (`terminalCommitted` guard in AgentActor).
- Do not use `OrchestratorEvent` for actor-to-actor communication.
- Events should be serializable and stable enough for Host/TUI/RPC/debugging.
