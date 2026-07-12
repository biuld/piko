# Single-Agent Actor Runtime Design

> Status: accepted technical design (single-agent product path landed)
> Business model: [Single-Agent Runtime Model](single-agent-runtime-model.md)
> Migration plan: [Single-Agent Runtime Migration](single-agent-runtime-migration.md)
> Landing checklist: [Single-Agent Runtime Landing Plan](single-agent-runtime-landing.md)
>
> Implementation names: `orchd::AgentExecutionRuntime` implements
> `orchd-api::AgentExecutor`. Design text below still says `AgentRuntime` /
> `AgentRuntimeApi` for the same facade role.

## 1. Purpose

This document maps the single-agent business model onto Tokio using an Actor
model. It defines runtime ownership, Actor boundaries, Tokio task lifecycle,
mailbox protocols, acknowledgement, persistence, observation, cancellation,
supervision, finalization, locking, backpressure, and shutdown.

The central rule is:

> Mutable business state is exclusively owned by its Aggregate Actor. Other
> components interact through typed messages and explicit acknowledgements.

Actor boundaries follow business consistency boundaries. A mutex, future,
Model Step, or tool call does not become an Actor merely because it is
concurrent.

## 2. Runtime Topology

```text
hostd
  └─ SessionDirectory
      └─ SessionActor[session_id]
          ├─ Session and Turn state
          ├─ durable writer
          ├─ live projection
          ├─ follow-up queue
          ├─ approval/interaction state
          └─ CommittedEventLog
                  │
                  │ start/cancel/query
                  ▼
orchd
  └─ AgentRuntime
      └─ SessionExecutionScope[session_id]
          └─ ExecutionActor[execution_id]
              ├─ ExecutionState
              ├─ ModelStepRunner
              ├─ ToolExecutor
              └─ ExecutionFinalizer
```

There are two core business Actors:

- `SessionActor` in hostd owns one Conversation Session and its Interaction
  Turns.
- `ExecutionActor` in orchd owns one active Agent Execution.

`AgentRuntime` is the orchd public service and Execution Actor supervisor. It is
not itself a mailbox Actor.

## 3. Actor Boundaries

| Component | Actor | Reason |
|---|---:|---|
| `SessionActor` | yes | owns durable Session/Turn invariants across commands |
| `ExecutionActor` | yes | owns long-running execution state and receives control |
| `AgentRuntime` | no | facade, scope registry, spawn, supervision, and reaping |
| `SessionExecutionScope` | no | immutable capabilities and Actor address table |
| `ModelStepRunner` | no | one short-lived provider operation |
| `ToolExecutor` | no | scoped child futures owned by an ExecutionActor |
| llmd gateway | no | service for one Model Step stream |
| committed event log | no separate Actor | serialized by SessionActor ownership |
| realtime delta hub | no | lossy fan-out without business state |

## 4. AgentRuntime

`AgentRuntime` is the only public orchd runtime entry point.

```rust
pub struct AgentRuntime {
    services: Arc<ProcessServices>,
    sessions: RwLock<HashMap<SessionId, Arc<SessionExecutionScope>>>,
    actors: Mutex<JoinSet<ExecutionExit>>,
    accepting: AtomicBool,
}
```

Responsibilities:

- attach and detach `SessionExecutionScope`;
- validate Session and Execution identity;
- enforce active Execution concurrency policy;
- create bounded mailboxes and cancellation tokens;
- spawn and supervise `ExecutionActor` tasks;
- route steer, cancel, and snapshot requests;
- convert panic or abnormal task exit into an Execution outcome;
- invoke terminal finalization;
- remove handles with generation-safe cleanup;
- coordinate graceful shutdown.

`AgentRuntime` does not own transcript mutation, Execution business state,
Session or Turn state, JSONL storage, approval decisions, or Model Step logic.

### 4.1 Public API

```rust
#[async_trait]
pub trait AgentRuntimeApi: Send + Sync {
    async fn attach_session(
        &self,
        config: SessionExecutionConfig,
    ) -> Result<SessionExecutionHandle, RuntimeError>;

    async fn detach_session(
        &self,
        session_id: SessionId,
    ) -> Result<(), RuntimeError>;

    async fn start_execution(
        &self,
        request: StartExecutionRequest,
    ) -> Result<ExecutionReceipt, RuntimeError>;

    async fn steer_execution(
        &self,
        request: SteerExecutionRequest,
    ) -> Result<InputReceipt, RuntimeError>;

    async fn request_cancel(
        &self,
        request: CancelExecutionRequest,
    ) -> Result<CancelReceipt, RuntimeError>;

    async fn execution_snapshot(
        &self,
        session_id: SessionId,
        execution_id: ExecutionId,
    ) -> Result<Option<ExecutionSnapshot>, RuntimeError>;
}
```

`start_execution()` waits for validation, registry reservation, Actor spawn,
and startup registration. It does not wait for the Execution to finish.

## 5. SessionExecutionScope

`SessionExecutionScope` is an orchd capability boundary, not a business Session.

```rust
struct SessionExecutionScope {
    session_id: SessionId,
    ports: Arc<SessionExecutionPorts>,
    executions: Mutex<HashMap<ExecutionId, ExecutionHandle>>,
    generation: AtomicU64,
}

struct SessionExecutionPorts {
    commit: Arc<dyn ExecutionCommitPort>,
    approval: Arc<dyn ApprovalPort>,
    interaction: Arc<dyn InteractionPort>,
    realtime: Arc<dyn RealtimeDeltaSink>,
}
```

Ports are immutable for the scope lifetime. There is no global persist sink,
approval gateway, or callback rebinding.

Single-agent mode permits at most one active root Execution. The registry stays
map-shaped so future child Executions do not require changing identity or
routing.

## 6. SessionActor

Each open hostd Session has one `SessionActor`.

```rust
struct SessionActor {
    state: SessionState,
    store: SessionStore,
    committed: CommittedEventLog,
    mailbox: mpsc::Receiver<SessionCommand>,
    runtime: Arc<dyn AgentRuntimeApi>,
}
```

It exclusively owns:

- Session metadata and selected branch;
- transcript and message heads;
- Interaction Turn state and Turn-to-Execution binding;
- durable sequence/revision allocation;
- live Session projection;
- queued follow-up input;
- pending approval and interaction state;
- committed reliable event insertion.

Different SessionActors execute concurrently. Commands within one Session are
serialized.

### 6.1 Session mailbox

```rust
enum SessionCommand {
    SubmitTurn {
        request: SubmitTurnRequest,
        reply: oneshot::Sender<Result<TurnReceipt, SessionError>>,
    },
    CommitMessage {
        commit: MessageCommit,
        reply: oneshot::Sender<Result<CommitAck, CommitError>>,
    },
    CommitExecutionOutcome {
        commit: ExecutionOutcomeCommit,
        reply: oneshot::Sender<Result<CommitAck, CommitError>>,
    },
    RequestCancelTurn {
        turn_id: TurnId,
        reply: oneshot::Sender<Result<CancelReceipt, SessionError>>,
    },
    QueueFollowUp {
        input: PendingTurnInput,
        reply: oneshot::Sender<Result<QueueReceipt, SessionError>>,
    },
    ResolveApproval {
        response: ApprovalResponse,
        reply: oneshot::Sender<Result<(), SessionError>>,
    },
    Snapshot {
        reply: oneshot::Sender<SessionSnapshot>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}
```

The mailbox is bounded. Capacity is explicit runtime configuration. Overload is
an error rather than unbounded memory growth.

## 7. ExecutionActor

One Agent Execution is implemented by one short-lived Tokio Actor.

```rust
struct ExecutionActor {
    identity: ExecutionIdentity,
    state: ExecutionState,
    mailbox: mpsc::Receiver<ExecutionCommand>,
    cancel: CancellationToken,
    ports: Arc<SessionExecutionPorts>,
    services: Arc<ProcessServices>,
    snapshot_tx: watch::Sender<ExecutionSnapshot>,
}

struct ExecutionState {
    status: ExecutionStatus,
    transcript: TranscriptManager,
    model_step_index: u32,
    steering: VecDeque<PendingInput>,
    pending_tools: Option<PendingToolBatch>,
    usage: Usage,
}
```

Only the Actor task mutates `ExecutionState`. It is not wrapped in
`Arc<Mutex<_>>` and is never exposed through `ExecutionHandle`.

### 7.1 Execution mailbox

```rust
enum ExecutionCommand {
    Steer {
        request: SteerExecutionRequest,
        reply: oneshot::Sender<Result<InputReceipt, ExecutionError>>,
    },
    Cancel {
        request_id: RequestId,
        reason: CancelReason,
        reply: oneshot::Sender<Result<CancelReceipt, ExecutionError>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}
```

The mailbox is bounded. Steering applies backpressure. Cancellation also
triggers the Actor's `CancellationToken`, allowing provider/tool waits to stop
without waiting for a mailbox boundary.

### 7.2 Execution handle

```rust
#[derive(Clone)]
struct ExecutionHandle {
    identity: ExecutionIdentity,
    generation: u64,
    command_tx: mpsc::Sender<ExecutionCommand>,
    cancel: CancellationToken,
    snapshot_rx: watch::Receiver<ExecutionSnapshot>,
}
```

The handle is an Actor address plus read-only observation. It contains no
mutable transcript or business state.

## 8. Execution Loop

The Actor runs one complete Agent Execution and then exits.

```text
initialize from committed context
  → publish Running snapshot
  → run Model Step
  → commit assistant Message
  → execute and commit tool results when present
  → accept steering at Model Step boundary
  → continue or return terminal ExecutionOutcome
```

The Actor body returns one `ExecutionOutcome`; it does not publish terminal
lifecycle events from individual branches.

```rust
async fn run(mut self) -> Result<ExecutionOutcome, ExecutionError> {
    self.transition_running();

    loop {
        if self.cancel.is_cancelled() {
            return Ok(ExecutionOutcome::Cancelled);
        }

        let step = self.run_model_step_interruptibly().await?;
        self.commit_assistant(&step).await?;

        if !step.tool_calls.is_empty() {
            self.execute_and_commit_tools(step.tool_calls).await?;
        }

        self.drain_controls_at_step_boundary().await?;

        if self.has_pending_steering() {
            self.commit_next_steering().await?;
            continue;
        }

        if step.requires_next_model_step() {
            continue;
        }

        return Ok(ExecutionOutcome::Succeeded {
            usage: self.state.usage.clone(),
        });
    }
}
```

The supervisor converts a returned `ExecutionError` into
`ExecutionOutcome::Failed`. The invariant is one resulting outcome and one
supervisor-owned finalizer.

## 9. ModelStepRunner

`ModelStepRunner` is an async operation owned by the ExecutionActor.

```rust
async fn run_model_step(
    input: ModelStepInput,
    cancel: CancellationToken,
) -> Result<ModelStepResult, ModelStepError>;

struct ModelStepResult {
    assistant: Message,
    tool_calls: Vec<ToolCall>,
    stop_reason: StopReason,
    usage: Usage,
}
```

It consumes one llmd `GatewayEvent` stream and publishes realtime deltas while
assembling the final assistant Message.

Cancellation is propagated through a token and cancellation-aware selection.
Steering received during the Model Step is queued and applied only after that
step's tool batch completes.

## 10. ToolExecutor

Tool calls are short-lived child futures, not Actors.

`JoinSet` or `FuturesUnordered` may execute allowed tools concurrently.

Rules:

1. realtime tool completion may publish in completion order;
2. durable tool-result Messages enter transcript in assistant source order;
3. child futures inherit cancellation;
4. all child futures complete or abort before the batch scope exits;
5. tool futures never mutate transcript directly;
6. only ExecutionActor commits ordered results and changes state.

## 11. Command Acknowledgement

Mailbox delivery and business acknowledgement are different.

Typed commands use `mpsc + oneshot`:

```text
caller sends command
  → Actor validates command
  → Actor performs required durable transition
  → Actor updates snapshot
  → Actor resolves oneshot reply
```

Durable commands await commit acknowledgement. Cancellation exposes two waits:

```text
request_cancel().await   Actor accepted cancellation
wait_terminal().await    Execution reached durable terminal outcome
```

Strong queries use request/reply. High-frequency UI state uses a
`watch::Receiver<Snapshot>`. Realtime notification does not await consumers.

## 12. Persistence

Persistence is a transactional Actor call, not an output Stream.

```rust
trait ExecutionCommitPort: Send + Sync {
    async fn commit_message(
        &self,
        commit: MessageCommit,
    ) -> Result<CommitAck, CommitError>;

    async fn commit_execution_outcome(
        &self,
        commit: ExecutionOutcomeCommit,
    ) -> Result<CommitAck, CommitError>;
}
```

The host implementation sends `SessionCommand` and awaits its reply:

```text
ExecutionActor
  → ExecutionCommitPort
  → SessionActor mailbox
  → append durable record
  → update Session projection
  → append CommittedEvent
  → CommitAck
```

SessionActor serializes ordering for one Session. Durable sequence allocation
belongs to hostd.

## 13. Committed Events

Reliable production uses an event sink:

```rust
trait CommittedEventSink {
    fn append(
        &mut self,
        event: CommittedEvent,
    ) -> Result<EventCursor, EventError>;
}
```

Consumers use a source that creates a Stream:

```rust
trait CommittedEventSource {
    async fn subscribe(
        &self,
        after: Option<EventCursor>,
    ) -> Result<CommittedEventStream, SubscribeError>;
}
```

Only a subscription result is a Stream. Actors write through sinks.

SessionActor performs in order:

```text
durable append
→ live projection update
→ CommittedEventSink.append
→ CommitAck
```

A committed event therefore means the fact is already durable and visible in
hostd projection.

## 14. Realtime Delta

Realtime production and consumption are separate interfaces:

```rust
trait RealtimeDeltaSink {
    fn try_publish(&self, delta: RealtimeDeltaEnvelope);
}

trait RealtimeDeltaSource {
    fn subscribe(&self) -> RealtimeDeltaStream;
}
```

The sink is lossy and must not block ExecutionActor. Realtime deltas are not
durable, do not acknowledge commands, do not drive lifecycle, and may be
dropped under lag.

## 15. Observation Lanes

Committed events and realtime deltas are separate QoS lanes.

| Property | Committed event | Realtime delta |
|---|---|---|
| producer API | sink `append` | sink `try_publish` |
| consumer API | cursor stream | lossy stream |
| retained | yes | no or minimal |
| replay | within retention | no |
| lag behavior | snapshot required | drop |
| recovery source | durable record, not stream | never |
| Actor backpressure | commit path only | never |

The preferred subscription keeps both lanes typed:

```rust
struct SessionSubscription {
    committed: CommittedEventStream,
    realtime: RealtimeDeltaStream,
}
```

An outer adapter may merge them into a convenience enum, but there is no global
order between lanes.

## 16. Cross-Actor Call Rules

Actor calls must not form await cycles.

Forbidden:

```text
SessionActor awaits ExecutionActor completion
  while ExecutionActor awaits SessionActor commit
```

Rules:

1. Actor mailboxes are bounded.
2. An Actor may await infrastructure it exclusively owns when serialization is
   required for correctness.
3. An Actor must not block its mailbox awaiting another Actor that may call it.
4. Cross-Actor long operations use operation identity and completion messages.
5. A calls B synchronously only when B cannot call A before replying.
6. Actor termination never depends on subscriber consumption.

### 16.1 Turn start without an await cycle

SessionActor commits the user Message before spawning ExecutionActor:

```text
SessionActor
  → persist TurnSubmitted + user Message
  → AgentRuntime.start_execution(committed context)
  → receive spawn/registration receipt
  → persist/project TurnRunning
```

`start_execution()` does not call SessionActor before returning registration.
Later Execution commits use the normal SessionActor commit port.

## 17. Supervision and Finalization

AgentRuntime wraps every ExecutionActor body:

```rust
async fn supervise_execution(
    scope: Arc<SessionExecutionScope>,
    actor: ExecutionActor,
    generation: u64,
) -> ExecutionExit {
    let identity = actor.identity().clone();

    let outcome = match AssertUnwindSafe(actor.run()).catch_unwind().await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => ExecutionOutcome::failed(error),
        Err(panic) => ExecutionOutcome::failed(panic_to_execution_error(panic)),
    };

    let terminal = finalize_execution(&scope, &identity, outcome).await;
    scope.remove_if_generation(&identity.execution_id, generation).await;
    ExecutionExit { identity, terminal }
}
```

AgentRuntime owns panic conversion, terminal commit, completion waiter
resolution, and generation-safe cleanup. Finalization depends only on immutable
identity, scoped ports, and outcome—not mutable Actor state lost after panic.

Durable exactly-once is enforced by SessionActor using `execution_id`:

- identical terminal replay is idempotent;
- conflicting terminal outcome is an invariant error.

## 18. Cancellation

Cancellation combines:

- a mailbox command that records intent and replies with acceptance;
- a `CancellationToken` that interrupts provider, tool, approval, and
  interaction waits.

```rust
tokio::select! {
    result = operation => result,
    _ = cancel.cancelled() => Err(ExecutionError::Cancelled),
}
```

Cancellation always passes through normal finalization. Dropping a future or
aborting a Tokio task is not a business terminal outcome.

## 19. Locking Rules

1. Actor-owned business state is not placed behind a shared mutex.
2. No registry lock is held across external await.
3. Registry locks cover only attach, reserve, find, and remove.
4. No lock is held while calling llmd, tools, commit, approval, or interaction
   ports.
5. SessionExecutionScope ports are immutable after attach.
6. Tool child futures cannot mutate transcript.
7. Slow subscribers cannot block commit or Execution progress.
8. Cleanup checks generation so an old Actor cannot remove a newer handle.

## 20. Backpressure

Capacities are explicit configuration:

- SessionActor mailbox;
- ExecutionActor mailbox;
- realtime fan-out;
- committed-event retention;
- maximum concurrent tools;
- future maximum child Executions.

Under pressure:

- durable commands wait for bounded capacity or return overload;
- steering returns overload instead of growing unbounded;
- cancel triggers its token even when normal traffic is congested;
- realtime deltas drop;
- subscribers behind retention receive snapshot-required;
- tool concurrency remains capped.

## 21. Shutdown

### 21.1 Session shutdown

```text
stop accepting new Turns
→ request active Execution cancellation
→ await terminal outcome with deadline
→ persist interrupted failure when terminal cannot be obtained
→ flush Session storage
→ stop SessionActor
→ detach SessionExecutionScope
```

### 21.2 AgentRuntime shutdown

```text
accepting = false
→ reject new Executions
→ cancel all ExecutionActors
→ await JoinSet with deadline
→ abort remaining Tokio tasks
→ attempt infrastructure-failure finalization
→ clear scopes
```

Forced Tokio abort is a last resort and becomes interrupted Execution recovery
state.

## 22. Multi-Agent Extension

The Actor model extends by adding ExecutionActors to one scope:

```text
AgentRuntime
  └─ SessionExecutionScope
      ├─ root ExecutionActor
      ├─ attached child ExecutionActor
      └─ detached child ExecutionActor
```

AgentRuntime remains supervisor and registry. Parent Actors hold child identity
and completion policy, not mutable child state or direct object references.

An attached-child barrier extends root finalization. Detached child Actors
continue independently. ModelStepRunner and ToolExecutor semantics do not
change.

## 23. Technical Invariants

1. One ExecutionActor is the sole writer of one Execution's mutable state.
2. One SessionActor is the sole writer of one Session and its Turns.
3. AgentRuntime supervises ExecutionActors but owns no Execution business state.
4. Every Actor mailbox is bounded.
5. Every correctness-relevant command has a typed acknowledgement.
6. Mailbox delivery is not business acknowledgement.
7. Persistence is request/ack, not an output Stream.
8. Reliable producers write to `CommittedEventSink`; subscribers read a
   `CommittedEventStream`.
9. Realtime producers use a lossy sink and never await consumers.
10. Public observation never drives Actor state.
11. Actor-to-Actor await cycles are forbidden.
12. Every ExecutionActor exit passes through one AgentRuntime finalizer.
13. Subscriber disconnect never cancels an ExecutionActor.
14. Tokio task abort alone never represents successful business termination.
15. Multi-agent support adds ExecutionActors without changing root Execution,
    Model Step, or SessionActor semantics.

## 24. Verification Strategy

### Actor ownership

- concurrent commands produce serialized state transitions;
- no public handle exposes mutable Actor state;
- old generations cannot clean up newer Actors;
- mailbox overload is deterministic.

### Acknowledgement

- send success is not returned as durable success;
- CommitAck follows durable append, projection, and committed-event append;
- cancel acceptance and terminal cancellation are independently observable.

### Supervision

- normal success finalizes once;
- provider, tool, and persistence failures finalize once;
- panic is converted and finalized;
- shutdown cancellation is finalized;
- forced abort recovers as interruption.

### Observation

- committed-event lag produces snapshot-required;
- realtime lag drops without blocking Actor progress;
- subscriber disconnect does not stop execution;
- merged observation assumes no cross-lane order.

### Deadlock prevention

- SessionActor commits Execution messages while a Turn is Running;
- SessionActor never awaits full Execution completion in a mailbox handler;
- ExecutionActor awaits SessionActor commit without reverse synchronous
  dependency;
- shutdown completes with full mailboxes and pending approval/tool waits.
