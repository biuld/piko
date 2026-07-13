# Agent Runtime Actor Design

> Status: current normative technical design
> Business concepts: [Single-Agent Runtime Model](single-agent-runtime-model.md)
> Multi-agent extension: [Multi-Agent Runtime Model](multi-agent-execution-model.md)
> Protocol resource control: [Actor Protocol Scopes Design](actor-protocol-scopes-design.md)

## 1. Design Boundary

piko exposes Agents, not Executions.

```text
User / LLM / hostd use cases
  └─ Session + AgentInstance + input/report/inbox

orchd implementation
  └─ AgentRuntime
      └─ AgentActor
          └─ internal ExecutionActor?
              └─ Model Step
                  └─ Tool Execution
```

`Execution` remains necessary for one short-lived agent loop, cancellation,
commit identity, realtime routing, usage, and diagnostics. It is not a public
control address. There is no public `AgentExecutor`, `start_execution`,
`wait_execution`, or `cancel_execution` API.

## 2. Process Topology

```text
hostd
  ├─ Conversation Session and Turn state
  ├─ session schema-v3 storage
  ├─ prompt/spec/config resolution
  ├─ approval and interaction delivery
  └─ user-visible committed projection
          │
          │ AgentRuntimeApi
          ▼
orchd AgentRuntime
  └─ SessionAgentScope[session_id]
      ├─ root AgentActor
      ├─ child AgentActor*
      └─ internal execution capabilities
```

hostd is authoritative for user-visible state. orchd owns live Agent and
Execution coordination. llmd owns model-provider interaction.

## 3. Actor Boundaries

### 3.1 AgentActor

One long-lived AgentActor exclusively owns one AgentInstance's mutable state:

- immutable identity and resolved AgentSpec;
- lifecycle (`Open`, `Closed`, `Unavailable`, `Terminated`);
- private working transcript;
- at most one active internal Execution;
- queued follow-up inputs;
- completed execution reports;
- waiters for an internally active run;
- detached report delivery registrations;
- durable inbox projection.

All changes to those fields pass through its bounded mailbox. Callers never
receive an Actor handle or mailbox sender.

### 3.2 ExecutionActor

One short-lived ExecutionActor owns exactly one internal run:

- Model Step loop;
- steering queue;
- cancellation token;
- current transcript context;
- tool batch execution;
- usage and terminal outcome;
- committed-message and realtime-delta emission.

ExecutionActor does not create or address other Agents. A multi-agent tool is
an ordinary tool call whose provider calls AgentRuntime.

### 3.3 SessionAgentScope

SessionAgentScope is a registry and capability boundary, not a business Actor.
It owns root identity, AgentActor handles, generation-safe cleanup, create
idempotency, relationship policy, limits, and immutable host-owned capabilities.
It contains no independent transcript or result cache.

### 3.4 AgentRuntime

AgentRuntime is the mandatory facade and supervisor. It attaches Session scopes,
creates durable AgentInstances before routing them, validates relationships,
routes Agent operations, supervises Actors, owns internal Execution identity,
and coordinates cancellation and shutdown.

It is not itself a mailbox Actor and does not duplicate AgentActor state.

## 4. Public Agent API

```rust
trait AgentRuntimeApi {
    async fn attach_agent_session(config: SessionAgentConfig) -> SessionAgentHandle;
    async fn detach_agent_session(session_id: SessionId);

    async fn create_agent(request: CreateAgentRequest) -> CreateAgentReceipt;
    async fn send_agent_input(request: SendAgentInputRequest) -> AgentInputReceipt;
    async fn run_agent(request: SendAgentInputRequest) -> AgentExecutionReport;
    async fn send_agent_input_detached(
        request: SendAgentInputRequest,
        recipient_agent_instance_id: AgentInstanceId,
    ) -> AgentInputReceipt;
    async fn steer_agent(request: SteerAgentRequest) -> AgentInputReceipt;
    async fn cancel_agent_run(session_id: SessionId, agent: AgentInstanceId);

    async fn close_agent(request: AgentLifecycleRequest);
    async fn reopen_agent(request: AgentLifecycleRequest);
    async fn agent_snapshot(session_id: SessionId, agent: AgentInstanceId);
    async fn list_agents(session_id: SessionId);
    async fn agent_inbox(session_id: SessionId, agent: AgentInstanceId);
    async fn consume_agent_inbox_item(request: ConsumeAgentInboxRequest);
}
```

Internal receipts and durable reports may retain an Execution ID for storage
and diagnostics. Multi-agent tool inputs and results explicitly project it
away. Neither the user nor the model can use it as an address.

## 5. Root Turn Flow

```text
TUI TurnSubmit
  → hostd creates and commits Turn/user input
  → hostd calls AgentRuntime.run_agent(root AgentInstance)
  → root AgentActor starts one internal ExecutionActor
  → ExecutionActor runs Model Steps and tools
  → durable terminal report returns to AgentActor
  → run_agent returns an Agent report
  → hostd finalizes the Turn and projects user-visible state
```

hostd tracks the active `turn_id`, not an active Execution address. Steering
and cancellation target the root AgentInstance.

## 6. Input Semantics

| Agent state | Delivery | Semantics |
|---|---|---|
| Idle | `StartWhenIdle` | start a new internal Execution |
| Idle | `SteerActive` | reject |
| Running | `StartWhenIdle` | reject busy |
| Running | `SteerActive` | route to active internal Execution |
| Running | `FollowUp` | enqueue for a later run |
| Closed | any | reject until reopened |

`run_agent` registers its waiter inside AgentActor before the terminal mailbox
message can be processed. Detached delivery is registered in the same mailbox
command that accepts input, so completion cannot race a later tracking call.

## 7. Persistence and Observation

hostd supplies `AgentCommitPort` for lifecycle/reports/inbox and an internal
`ExecutionCommitPort` for transcript and terminal commits. The latter is a
capability, not an Execution control API.

Ordering rules:

1. Agent creation commits before registry insertion.
2. Transcript commits are acknowledged before reliable observation.
3. Realtime deltas are lossy and never gate progress.
4. A terminal report commits before waiter or inbox publication.
5. schema v3 stores transcripts under `agents/<agent_instance_id>.jsonl`.

Observation has two lanes:

```text
reliable: durable commit → committed event → hostd projection
realtime: best-effort delta → TUI draft
```

Subscriber loss never cancels an Agent run. Internal Execution metadata may
appear in envelopes for ordering and diagnostics, never as a control address.

## 8. Cancellation

```text
cancel_agent_run(agent_instance_id)
  → resolve AgentActor and its active internal Execution
  → cancellation token + mailbox notification
  → durable Cancelled outcome
  → terminal Agent report
```

Attached parent cancellation cancels the child Agent's current run. Detached
children survive parent cancellation by default.

## 9. Supervision, Backpressure, and Shutdown

- Actor panic becomes a durable unavailable/failed state where possible.
- Tokio task abort is never successful business completion.
- mailboxes are bounded; full mailboxes return overload.
- no Tokio lock is held across model, tool, approval, or persistence awaits.
- no synchronous cross-Actor await cycle is allowed.
- Session detach cancels active runs, drains ExecutionActors, then stops Agents.
- registry removal is generation-checked.

## 10. Run Atomicity

Run startup uses `prepare → durable start → activate`. A failed start commit
rolls back the prepared Execution reservation, while failure after the durable
start converges through an ordinary failed terminal report.

Terminal completion enters `Finalizing`. Transcript advancement, waiter
resolution, detached delivery, and follow-up advancement occur only after the
durable terminal commit acknowledges. Durable follow-up input is atomically
removed from its queue when its run record is created.

The complete protocol and failure matrix are defined in
[Agent Run Atomicity Design](agent-run-atomicity-design.md).

## 11. Invariants

1. User and LLM operations address AgentInstances only.
2. Every attached AgentInstance has exactly one owning AgentActor.
3. One AgentActor has at most one active internal ExecutionActor.
4. Execution identity is never a multi-agent tool argument or result.
5. Agent creation is durable before routable.
6. Terminal reports are durable before waiter/inbox publication.
7. Detached completion is registered atomically with input acceptance.
8. Private transcripts never share mutable state.
9. hostd remains authoritative for user-visible Session and Turn state.
10. ExecutionActor remains an implementation detail of AgentRuntime.
