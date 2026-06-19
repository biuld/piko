# Events And State

`OrchestratorEvent` is the public fact stream. `InMemoryEventStore` owns event
ingestion, event log, reducer projection, subscriptions, snapshots, and graph
projection. It is a plain synchronous class — not an actor.

## Event Model

```mermaid
flowchart LR
  Message[AgentActor message]
  Work[Agent work]
  Event[OrchestratorEvent\nfact after transition]
  Store[InMemoryEventStore\nevent log owner]
  Reducer[reduceStateEvent()\npure projection]
  State[StateActorState]

  Message --> Work --> Event --> Store --> Reducer --> State
```

Business actors publish events with an injected async emitter:

```ts
interface AgentActorDeps {
  emit(event: OrchestratorEvent): Promise<void>;
}
```

The emitter is wired directly to `eventStore.append()`:

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
5. convert to HostEvent and notify all subscribers synchronously
6. return envelope
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
  | { type: "orchestrator_started" }
  | { type: "orchestrator_stopped"; reason?: string }
  | { type: "agent_registered"; agent: AgentSpec }
  | { type: "agent_unregistered"; agentId: string }
  | { type: "task_created"; task: AgentTask }
  | { type: "task_started"; agentId: string; taskId: string }
  | { type: "task_delta"; agentId: string; taskId: string; delta: AgentDelta }
  | { type: "task_completed"; agentId: string; taskId: string; result: AgentTaskResult }
  | { type: "task_failed"; agentId: string; taskId: string; error: string }
  | { type: "task_cancelled"; agentId: string; taskId: string; reason?: string }
  | { type: "task_transcript_committed"; agentId: string; taskId: string; messages: Message[]; summary: string; finalStatus: string }
  | { type: "plan_updated"; agentId: string; taskId: string; plan: AgentPlan }
  | { type: "tool_started"; agentId: string; taskId: string; callId: string; name: string }
  | { type: "tool_finished"; agentId: string; taskId: string; callId: string; result: unknown }
  | { type: "approval_requested"; approval: ApprovalRequest }
  | { type: "approval_resolved"; approvalId: string; decision: ApprovalDecision };
```

## Snapshot And Graph

`snapshot()` returns a deep clone of the current reducer projection:

```ts
interface OrchState {
  runId: string;
  status: "idle" | "running" | "stopping" | "stopped";
  agents: Record<string, AgentRuntimeState>;
  tasks: Record<string, AgentTaskState>;
  approvals?: Record<string, ApprovalRequestState>;
}
```

`graph()` returns nodes and edges derived from the current state:

- agent nodes
- task nodes
- approval nodes
- edges for running task, waiting approval, delegation

Graph rendering is a pure projection. It does not affect scheduling.

Plans are task state, not global state:

```ts
interface AgentTaskState {
  id: string;
  agentId: string;
  status: AgentTaskStatus;
  plan?: AgentPlan;
}
```

## Event Usage By Component

| Component | Emits |
| --- | --- |
| `Orchestrator` facade | `orchestrator_started`, `task_created`, `agent_registered`, `agent_unregistered` |
| `AgentActor` | `task_started`, `task_delta`, `plan_updated`, `task_transcript_committed`, `task_completed`, `task_failed`, `task_cancelled` |
| `ToolRegistryImpl.executeTool()` | `tool_started`, `tool_finished`, `approval_requested`, `approval_resolved` |

Constraints:

- Emit only after the component's private state transition has happened.
- Emit terminal task events exactly once (`terminalCommitted` guard in AgentActor).
- Do not use `OrchestratorEvent` for actor-to-actor communication.
- Events should be serializable and stable enough for Host/TUI/RPC/debugging.
