# Multi-Agent Runtime Migration

> Status: implemented and workspace-validated
> Target model: [Multi-Agent Runtime Model](multi-agent-execution-model.md)
> Single-agent base: [Single-Agent Runtime Model](single-agent-runtime-model.md)
> Actor design: [Single-Agent Actor Runtime Design](single-agent-actor-runtime-design.md)

## 1. Goal

Extend the stable single-agent runtime into a multi-agent runtime built around:

```text
AgentRuntime
  └─ SessionAgentScope
      └─ AgentInstance Tree
          └─ AgentActor
              └─ active ExecutionActor?
```

The migration introduces long-lived AgentInstance identity and private
transcript without changing the established semantics of Interaction Turn,
Agent Execution, Model Step, Tool Execution, commit acknowledgement, committed
events, or realtime deltas.

Every multi-agent operation must pass through AgentRuntime. LLM access is
limited to typed tools backed by AgentRuntime.

## 2. Preconditions

Multi-agent implementation starts only after the single-agent path satisfies:

1. root Turns use Agent Execution rather than Task/Work lifecycle;
2. one ExecutionActor exclusively owns one Execution;
3. one SessionActor exclusively owns Session and Turn durable state;
4. persistence uses request/ack through SessionActor;
5. committed events and realtime deltas have separate sink/source contracts;
6. Execution finalization is unified and exactly-once;
7. cancellation and interrupted recovery converge deterministically;
8. session-scoped capabilities are immutable;
9. command acknowledgement does not depend on public observation;
10. subscriber disconnect does not affect Execution lifecycle.

Multi-agent work must not compensate for missing single-agent invariants with a
new Task compatibility layer.

This is an independent migration track. It is not Phase 7 of the single-agent
migration: the single-agent runtime is its prerequisite and remains a complete
product mode throughout this rollout.

## 3. Current-to-Target Mapping

| Existing concept | Target concept | Migration action |
|---|---|---|
| root single-agent runtime | root AgentInstance | materialize stable AgentInstance identity |
| legacy Task identity | AgentInstance identity where recoverable | read-only migration input, not new API |
| legacy Task runtime | AgentActor + short ExecutionActor | split long-lived and per-run ownership |
| AgentSpec registry | resolved AgentSpec snapshots | keep hostd authority; bind snapshot to AgentInstance |
| Task DAG | AgentInstance Tree | migrate identity/parent links only |
| Task transcript shard | AgentInstance private transcript | migrate storage/query semantics |
| Task result cache | Agent/Execution projection | delete independent mutable cache |
| TaskControlPort | AgentRuntime API | replace all create/send/steer/status routes |
| spawn tool provider | thin MultiAgentToolProvider | remove registry and lifecycle ownership |
| detached Task result | durable Agent inbox/report | replace poll-only cache behavior |
| Task status | Agent lifecycle + activity projection | split Open/Closed from Idle/Running |

## 4. Target Components

### 4.1 AgentRuntime

AgentRuntime becomes the mandatory facade, registry, policy boundary, router,
and Actor supervisor for both AgentInstances and Executions.

```rust
pub struct AgentRuntime {
    services: Arc<ProcessServices>,
    sessions: RwLock<HashMap<SessionId, Arc<SessionAgentScope>>>,
    agent_actors: Mutex<JoinSet<AgentExit>>,
    execution_actors: Mutex<JoinSet<ExecutionExit>>,
    accepting: AtomicBool,
}
```

### 4.2 SessionAgentScope

```rust
struct SessionAgentScope {
    session_id: SessionId,
    ports: Arc<SessionAgentPorts>,
    root_agent_instance_id: AgentInstanceId,
    agents: Mutex<HashMap<AgentInstanceId, AgentHandle>>,
    executions: Mutex<HashMap<ExecutionId, ExecutionHandle>>,
    generations: AtomicU64,
}
```

### 4.3 AgentActor

```rust
struct AgentActor {
    identity: AgentInstanceIdentity,
    spec: AgentSpec,
    transcript: AgentTranscript,
    inbox: VecDeque<AgentInboxItem>,
    follow_ups: VecDeque<PendingAgentInput>,
    active_execution: Option<ExecutionHandle>,
    latest_outcome: Option<ExecutionOutcomeSummary>,
    lifecycle: AgentInstanceLifecycle,
    mailbox: mpsc::Receiver<AgentCommand>,
}
```

### 4.4 MultiAgentToolProvider

```rust
struct MultiAgentToolProvider {
    runtime: Arc<dyn AgentRuntimeApi>,
}
```

It adapts tool arguments and trusted execution context to typed AgentRuntime
requests. It owns no registry, Actor, transcript, lifecycle, result cache, or
authorization policy.

## 5. Protocol Additions

Introduce stable identities:

```rust
type AgentInstanceId = String;
type AgentSpecId = String;
```

Add:

- `AgentInstanceIdentity`;
- `AgentInstanceLifecycle`;
- `AgentActivity`;
- `AgentSnapshot`;
- `AgentExecutionReport`;
- `CreateAgentRequest/Receipt`;
- `SendAgentInputRequest`;
- `SteerAgentRequest`;
- `CloseAgentRequest`;
- `ReopenAgentRequest`;
- `AgentReportAvailable`;
- `AgentInboxSnapshot` where required.

Extend Execution identity with:

```text
agent_instance_id
origin_execution_id       audit/idempotency only
origin_tool_call_id        audit/idempotency only
```

Do not add:

- `parent_execution_id`;
- Execution Tree DTOs;
- Execution dependency state;
- attached-child barrier DTOs;
- raw Actor handles to protocol.

## 6. Persistence Additions

SessionActor gains durable commands for:

```text
AgentInstanceCreated
AgentInstanceClosed
AgentInstanceReopened
AgentInstanceTerminated
AgentParentLinked
AgentMessageCommitted
AgentExecutionStarted
AgentExecutionTerminal
AgentReportCommitted
AgentInboxDelivered
AgentInboxConsumed
```

Required recovery queries:

```text
AgentInstances by Session
AgentInstance by agent_instance_id
children by parent_agent_instance_id
private transcript by agent_instance_id
Executions by agent_instance_id
latest outcome by agent_instance_id
unread reports by recipient agent_instance_id
```

Ordering is explicitly separate:

- per-Agent transcript sequence;
- per-Execution sequence;
- Session committed-event cursor.

Legacy Task records may be read as migration input. New multi-agent writes do
not reintroduce Task/Work lifecycle.

## 7. Phased Migration

### Phase 0: Contract Freeze and Feature Gate

- Accept the AgentInstance Tree model.
- Mark Execution Tree and direct Task reuse designs as rejected.
- Put multi-agent tools behind one runtime feature gate.
- Prevent new code from calling legacy TaskControlPort.
- Add architecture tests that forbid raw Actor mailbox exposure.

Exit criteria:

- AgentRuntime is the documented mandatory boundary;
- protocol review accepts identities and cardinalities;
- single-agent behavior remains unchanged with the feature disabled.

### Phase 1: AgentInstance DTOs and Durable Model

- Add AgentInstance protocol DTOs and committed events.
- Add SessionActor commands for Agent lifecycle and parent links.
- Add durable AgentInstance projection and private transcript routing.
- Allocate a stable root `agent_instance_id` for every Session.
- Associate root Executions with the root AgentInstance.
- Preserve legacy session read compatibility behind migration adapters.

Exit criteria:

- a single-agent Session reopens with the same root AgentInstance identity;
- AgentInstance creation is durable before it becomes routable;
- SessionActor remains the only durable writer.

### Phase 2: SessionAgentScope and Agent Registry

- Replace execution-only session scope with `SessionAgentScope`.
- Add Agent handle registry keyed by `agent_instance_id`.
- Store only Actor address, generation, and read-only snapshot receiver.
- Add parent/child tree projection.
- Add generation-safe AgentActor cleanup.
- Keep Execution registry separately keyed by `execution_id`.

Exit criteria:

- AgentSpec/display name cannot route runtime commands;
- duplicate IDs and cross-Session links are rejected;
- registry contains no mutable transcript or independent result cache.

### Phase 3: AgentActor and Root Integration

- Introduce AgentActor with private transcript, inbox, follow-up queue, lifecycle,
  and at most one active Execution.
- Route root user input through root AgentActor.
- Move transcript working-context handoff from Session/root glue into
  AgentActor-to-ExecutionActor startup.
- On Execution terminal, return outcome to owning AgentActor.
- Keep Interaction Turn ownership in SessionActor.

Exit criteria:

- root AgentInstance survives multiple Turns;
- every Turn creates a new root Execution;
- Execution failure leaves root AgentInstance reusable;
- AgentActor and ExecutionActor have no shared mutable state.

### Phase 4: AgentRuntime Unified API

- Implement `create_agent`.
- Implement `send_agent_input` with StartWhenIdle, SteerActive, and FollowUp.
- Implement `steer_agent`.
- Implement Execution cancellation routing.
- Implement close/reopen.
- Implement `agent_snapshot` and `list_agents`.
- Route hostd and internal callers through the same validation path.
- Make AgentActor mailbox private to AgentRuntime.

Exit criteria:

- no public API returns raw `mpsc::Sender` or Actor object;
- every command has typed acknowledgement semantics;
- all Agent/Execution commands validate Session and identity ownership.

### Phase 5: MultiAgentToolProvider and Attached Spawn

- Add trusted `ToolExecutionContext` containing Session, caller AgentInstance,
  caller Execution, and tool-call identity.
- Add thin `MultiAgentToolProvider`.
- Implement `spawn_agent` as:

```text
create child AgentInstance
→ send initial input
→ await first Execution terminal outcome
→ return AgentExecutionReport as tool result
```

- Propagate tool cancellation to the awaited child Execution.
- Enforce spawn idempotency using tool-call/request identity.
- Ensure the tool provider owns no lifecycle or result cache.

Exit criteria:

- ExecutionActor sees an ordinary tool future and ToolResult;
- no Execution dependency/barrier state exists;
- child failure becomes a bounded error report the parent model may handle;
- parent report Message is committed before the next Model Step.

### Phase 6: Agent Reuse and Communication Tools

- Implement `send_agent_message` for an existing AgentInstance.
- Support Idle → new Execution.
- Support Running → steer active Execution.
- Support Running → queue follow-up Execution.
- Implement `get_agent_status` from Agent projection.
- Implement close/reopen tools if allowed by capability policy.
- Add per-caller authorization for target Agent relationships.

Exit criteria:

- a child can be reused across multiple parent Turns/Executions;
- private transcript continuity is preserved;
- status does not depend on a transient poll cache;
- all Agent-to-Agent calls pass through AgentRuntime.

### Phase 7: Detached Spawn and Inbox

- Implement `spawn_agent_detached`.
- Return only after durable Agent creation and first Execution acceptance.
- Add durable `AgentExecutionReport` storage.
- Deliver terminal detached reports to parent Agent inbox.
- Publish `AgentReportAvailable` after durable inbox projection.
- Implement report collect/consume semantics.
- Add optional explicit `AutoFollowUp` policy; keep inbox as default.
- Ensure parent cancellation does not cancel detached child by default.

Exit criteria:

- detached result is never lost or silently injected into parent transcript;
- Session reopen restores unread reports;
- duplicate report delivery is idempotent;
- detached quotas are enforced before creation.

### Phase 8: Recovery, Shutdown, and Failure Hardening

- Reattach AgentActors from durable AgentInstance records.
- Restore private transcripts, lifecycle, latest outcomes, and inbox.
- Mark incomplete historical Executions interrupted.
- Cancel/drain active Executions before AgentActor shutdown.
- Stop AgentActors child-first on Session shutdown.
- Convert Actor panic and spawn-after-commit failure into durable unavailable or
  interrupted state.
- Test full mailbox, pending approval, pending tool, and runtime shutdown paths.

Exit criteria:

- Session recovery reproduces AgentInstance Tree and private transcripts;
- no started Agent/Execution disappears because a channel closed;
- shutdown reaches a bounded deterministic result.

### Phase 9: TUI and Host Projection

- Add AgentInstance list/tree projection keyed by `agent_instance_id`.
- Display AgentSpec name only as metadata.
- Show lifecycle separately from active Execution activity.
- Add unread detached-report indicator.
- Route Agent selection and input by AgentInstance identity.
- Keep Turn spinner bound only to the root Turn/root Execution.

Exit criteria:

- duplicate AgentSpec instances render as separate nodes;
- parent/child hierarchy derives only from `parent_agent_instance_id`;
- child/detached activity cannot complete or keep alive the wrong Turn.

### Phase 10: Legacy Multi-Agent Cleanup

- Delete TaskControlPort and legacy spawn/steer/poll paths.
- Delete Task DAG runtime projection from orchd.
- Delete Task result cache.
- Remove Task/Work multi-agent protocol variants.
- Remove direct Task mailbox routing.
- Retain only explicit read migration for legacy sessions.
- Remove feature gate after production validation.

Exit criteria:

- no product multi-agent path uses Task/Work identity;
- AgentRuntime is the only Agent/Execution control surface;
- workspace tests and lint pass without legacy multi-agent runtime code.

## 8. API Acknowledgement Semantics

| API | Successful acknowledgement means |
|---|---|
| `create_agent` | identity/link committed and AgentActor routable |
| `send_agent_input` | input accepted according to declared delivery policy |
| `steer_agent` | steering Message durably accepted for active Execution |
| `request_cancel_execution` | cancellation intent accepted, not yet terminal |
| `close_agent` | lifecycle commit complete and new input rejected |
| `reopen_agent` | lifecycle commit complete and input accepted again |
| `agent_snapshot` | projection read at Actor command boundary |
| detached spawn tool | Agent durable, Actor routable, first Execution accepted |
| attached spawn tool | first Execution terminal report returned as ToolResult |

Terminal wait remains separate from cancel acceptance.

## 9. Verification Matrix

### AgentInstance identity

| Scenario | Required result |
|---|---|
| root Session creation | stable durable root AgentInstance |
| create child | durable identity and parent link before routing |
| same AgentSpec twice | independent AgentInstance IDs |
| duplicate request | same receipt or explicit conflict |
| cross-Session parent | rejected |
| depth/count limit | rejected without partial Agent |

### AgentActor ownership

| Scenario | Required result |
|---|---|
| multiple input commands | serialized delivery |
| active Execution | at most one per AgentInstance |
| Execution terminal | AgentActor returns Idle and records outcome |
| Execution failure | Agent remains reusable |
| close/reopen | durable lifecycle and deterministic input policy |
| Actor panic | durable unavailable/interrupted projection |

### Private transcript

| Scenario | Required result |
|---|---|
| child creation | bounded filtered initial context |
| child continuation | uses its own committed transcript |
| parent mutation | cannot mutate child transcript |
| concurrent Agents | independent transcript ordering |
| reopen | exact transcript ownership restored |

### Attached spawn

| Scenario | Required result |
|---|---|
| success | report committed as parent tool result |
| business failure | error report, parent may continue |
| child panic | infrastructure-error report |
| parent/tool cancel | awaited child Execution cancelled |
| report commit failure | parent does not continue with phantom result |
| duplicate tool call | no duplicate AgentInstance or first Execution |

### Reuse and communication

| Scenario | Required result |
|---|---|
| send to Idle | new Execution |
| steer Running | active Execution receives boundary input |
| follow-up Running | later Execution queued |
| send Closed | rejected |
| unauthorized target | rejected by AgentRuntime |
| full mailbox | deterministic overload |

### Detached execution

| Scenario | Required result |
|---|---|
| spawn | immediate durable receipt |
| parent terminal | child continues |
| parent cancellation | child continues by default |
| terminal report | durable inbox item and committed notification |
| duplicate delivery | idempotent |
| reopen | unread report restored |
| Session shutdown | child cancelled/drained |

### Runtime boundary

| Scenario | Required result |
|---|---|
| forged caller tool args | ignored/rejected; trusted context wins |
| raw target Actor access | no public capability exists |
| tool provider registry mutation | impossible by interface |
| old generation exit | cannot remove newer Actor |
| shutdown under load | bounded convergence or explicit interruption |

## 10. Main Risks

### Reintroducing the old Task model

AgentInstance is intentionally long-lived, but it must not absorb Execution
status, Model Step state, or tool state. Agent lifecycle and Execution lifecycle
remain separate.

### AgentRuntime becoming a God Object

AgentRuntime owns routing, policy, registry, and supervision, but delegates
mutable business state to AgentActor and durable state to SessionActor. Internal
modules may split registry, policy, and supervision without exposing alternate
entry points.

### Cross-Actor await cycles

AgentRuntime registration must return without requiring a child callback to the
waiting parent. Attached waiting occurs in the spawn tool future. SessionActor
never waits synchronously for Agent/Execution completion while those Actors
need Session commits.

### Partial Agent creation

Durable identity commit and Actor spawn cannot be a database transaction. The
protocol must reserve identity, commit creation, spawn Actor, and explicitly
commit unavailable state if spawn fails.

### Transcript leakage

Context inheritance must be explicit, bounded, and security-filtered. Private
Agent transcript storage must never be inferred from AgentSpec or file name.

### Detached result loss

Detached reports require durable inbox delivery and idempotency. Public
observation alone is insufficient because subscribers may disconnect.

## 11. Rollout Strategy

Use a Session-level runtime mode, not mixed per-command routing:

```text
SingleAgent
MultiAgentV1
```

A Session selects one mode when attached to AgentRuntime. Do not execute legacy
Task and AgentInstance paths concurrently within one Session.

Recommended rollout:

1. test-only feature gate;
2. opt-in project setting;
3. new Sessions by default;
4. legacy Session migration after reopen validation;
5. remove old runtime path.

## 12. Completion Criteria

The migration is complete when:

1. AgentRuntime is the only Agent and Execution control surface;
2. AgentInstance Tree is durable and restored exactly;
3. AgentActor owns private transcript and serial input;
4. ExecutionActor remains topology-agnostic;
5. LLM multi-agent access exists only through typed tools;
6. attached spawn is a normal awaited Tool Execution;
7. detached reports are durable, discoverable, and idempotent;
8. child AgentInstances are reusable across Turns;
9. no Task DAG, Task result cache, or Execution dependency graph remains;
10. cancellation, panic, recovery, overload, and shutdown tests pass;
11. TUI routes and displays Agents by AgentInstance identity;
12. documentation and product APIs contain no conflicting Task/Work model.
