# Single-Agent Runtime Migration

> Status: proposed implementation plan
> Target model: [Single-Agent Runtime Model](single-agent-runtime-model.md)
> Technical design: [Single-Agent Actor Runtime Design](single-agent-actor-runtime-design.md)

## 1. Goal

Migrate orchd from a long-lived Task/Work runtime to the single-agent model:

```text
Session → Turn → Agent Execution → Model Step → Tool Execution
```

The migration preserves the existing Model Step, tool, persistence barrier, and
observation machinery where they already match the target model. It replaces
the Task/Work control plane and closes the hostd/orchd lifecycle protocol.

## 2. Current-to-Target Mapping

| Current term | Target | Action |
|---|---|---|
| Session | Conversation Session | keep; hostd-owned |
| Turn | Interaction Turn | keep; clarify user-visible meaning |
| root Task | no independent single-agent domain object | remove from public runtime model |
| Work | Agent Execution | replace and strengthen |
| Task step / StepCycle | Model Step | rename and retain behavior |
| Task mailbox | Execution control channel | limit to active Execution lifetime |
| Task registry | active Execution registry | reduce to live handles/projection |
| Task Idle | Session activity projection | derive from no active Execution |
| Task Failed | ExecutionFailed + Session Idle | remove duplicate lifecycle |
| Task Closed/Reopened | Session or feature-specific control | remove from orchd core |
| SessionOutputHub | Session observation channel | retain Event/Delta lanes |
| PersistSink | host commit capability | retain; scope by Session/Execution |

## 3. What Is Retained

The migration should preserve and refine:

- llmd `GatewayEvent` streaming;
- model configuration and provider routing;
- transcript assembly;
- assistant message assembly;
- tool discovery and execution;
- sequential and parallel tool batches;
- persistence-before-context barriers;
- reliable Event and best-effort Delta lanes;
- cancellation primitives;
- session cursor and reconnect behavior.

## 4. What Is Replaced

The migration replaces:

- `CreateTaskRequest`, `SubmitTaskInput`, and `TaskControlRequest` as the root
  Agent API;
- root Task creation and reuse;
- permanent Task mailbox loops;
- Task and Work duplicate state machines;
- Task DAG and result caches in the single-agent path;
- globally rebindable persistence, approval, and interaction ports;
- internal lifecycle observation as a second state writer;
- EventHub self-observation for command acknowledgement;
- snapshot polling for command completion.

## 5. Target Public API

The target orchd API is centered on Execution:

```rust
trait AgentExecutor {
    async fn start_execution(
        &self,
        request: StartExecutionRequest,
    ) -> Result<ExecutionReceipt, ExecutionError>;

    async fn steer_execution(
        &self,
        request: SteerExecutionRequest,
    ) -> Result<InputReceipt, ExecutionError>;

    async fn cancel_execution(
        &self,
        request: CancelExecutionRequest,
    ) -> Result<ExecutionSnapshot, ExecutionError>;

    async fn execution_snapshot(
        &self,
        session_id: SessionId,
        execution_id: ExecutionId,
    ) -> Result<Option<ExecutionSnapshot>, ExecutionError>;

    async fn subscribe_session(
        &self,
        request: SubscribeRequest,
    ) -> Result<SessionSubscription, ExecutionError>;
}
```

The first request shape is expected to include:

```rust
struct StartExecutionRequest {
    request_id: RequestId,
    session_id: SessionId,
    turn_id: TurnId,
    execution_id: ExecutionId,
    input_message_id: MessageId,
    input: MessageContent,
    context: ConversationContext,
    config: ExecutionConfig,
}
```

Session- and execution-scoped ports may be supplied through an attached session
capability instead of serialized request fields.

## 6. Target Runtime Components

### 6.1 AgentRuntime

Replace the broad Supervisor state with `AgentRuntime` as the public facade and
Execution Actor supervisor:

```rust
struct AgentRuntime {
    process_services: ProcessServices,
    sessions: RwLock<HashMap<SessionId, SessionExecutionEntry>>,
}

struct SessionExecutionEntry {
    ports: Arc<SessionExecutionPorts>,
    active: HashMap<ExecutionId, ActiveExecutionHandle>,
    output: Arc<SessionOutputHub>,
}
```

Single-agent mode enforces at most one active root Execution. The map shape is
retained so child Executions can be added later.

### 6.2 ExecutionRuntime

Replace the permanent Task runtime with a short-lived Execution runtime:

```rust
struct ExecutionRuntime {
    identity: ExecutionIdentity,
    context: ConversationContext,
    config: ExecutionConfig,
    state: ExecutionState,
    controls: Receiver<ExecutionControl>,
    services: ExecutionServices,
}
```

```rust
struct ExecutionState {
    status: ExecutionStatus,
    transcript: TranscriptManager,
    model_step_index: u32,
    pending_tools: Option<PendingToolBatch>,
    steering: VecDeque<PendingInput>,
    usage: Usage,
}
```

### 6.3 ExecutionFinalizer

Every terminal path calls one finalizer:

```text
stop accepting controls
  → settle or cancel pending tools
  → commit terminal Execution record
  → update live projection
  → publish reliable terminal event
  → complete command/result waiters
  → remove active handle
```

## 7. Protocol Changes

Replace Task/Work DTOs with:

- `ExecutionId`;
- `ExecutionStatus`;
- `ExecutionOutcome`;
- `StartExecutionRequest`;
- `ExecutionReceipt`;
- `SteerExecutionRequest`;
- `CancelExecutionRequest`;
- `ExecutionSnapshot`;
- `ExecutionEventEnvelope`.

Reliable identity becomes:

```text
session_id
turn_id
execution_id
execution_seq
```

Realtime identity becomes:

```text
session_id
execution_id
message_id
delta_seq
```

The protocol must distinguish `execution_seq` from the session observation
cursor for future concurrent child Executions.

## 8. Persistence Changes

Replace Task/Work lifecycle commits with Message and Execution commits:

```rust
trait ExecutionCommitPort {
    async fn commit_message(
        &self,
        commit: MessageCommit,
    ) -> Result<CommitAck, CommitError>;

    async fn commit_execution(
        &self,
        commit: ExecutionCommit,
    ) -> Result<CommitAck, CommitError>;
}
```

Commit identity includes:

```text
session_id
turn_id
execution_id
message_id when applicable
```

hostd owns durable ordering and returns the committed revision/sequence in the
acknowledgement.

The first implementation may write a legacy root shard internally for storage
compatibility. Legacy `task_id` must not leak into the new public API.

## 9. hostd Integration Changes

The target Turn path is:

```text
TurnSubmit
  → allocate turn_id + execution_id
  → persist TurnSubmitted
  → build ConversationContext and scoped capabilities
  → start_execution
  → persist/project TurnRunning after receipt
  → observe matching execution_id
  → commit exactly one terminal Turn outcome
```

Cancellation is:

```text
TurnCancel
  → cancel_execution(execution_id)
  → observe/commit ExecutionCancelled
  → commit TurnCancelled
```

hostd no longer waits for a combination of Task Idle and Work terminal.

## 10. Phased Migration

### Phase 0: Contract Freeze

- Adopt the target terminology and invariants.
- Freeze spawn, detached, poll, and cross-task steer development.
- Mark old Task/Work APIs as migration-only.
- Add architecture tests for identity and lifecycle cardinality.

Exit criteria:

- model and migration documents are accepted;
- no new feature depends on the old Task lifecycle.

### Phase 1: Execution DTOs and API

- Add Execution DTOs to protocol.
- Add the `AgentExecutor` API alongside the old Task API.
- Introduce immutable session/execution-scoped capabilities.
- Add direct command receipts independent of SessionOutput.

Exit criteria:

- Execution API compiles and has contract tests;
- no global port rebinding exists on the new path.

### Phase 2: ExecutionRuntime Vertical Slice

- Build `ExecutionRuntime` from the existing transcript, Model Step, tool, and
  event components.
- Add an active Execution registry.
- Implement the terminal finalizer.
- Support one Session with one root Execution.

Exit criteria:

- an in-memory prompt completes through multiple Model Steps;
- every terminal path removes its active handle exactly once.

### Phase 3: hostd Root Turn Integration

- Route normal `TurnSubmit` through `start_execution`.
- Persist exact Turn-to-Execution identity.
- Project committed Messages from HostState.
- Complete Turn only from the matching terminal Execution outcome.

Exit criteria:

- the TUI completes first and subsequent Turns without root Task reuse;
- no Task Idle/Work terminal inference remains on the new path.

### Phase 4: Control and Recovery

- Implement steering at Model Step boundaries.
- Implement hostd-owned follow-up queueing as new Turns.
- Connect `TurnCancel` to `cancel_execution`.
- Finalize interrupted historical Turns on Session open.
- Verify subscription reconnect does not affect execution.

Exit criteria:

- cancellation, retry, reconnect, and interruption tests pass;
- every accepted Turn reaches one terminal outcome.

### Phase 5: Remove Task/Work Runtime

- Delete the old Task commands and service surface.
- Delete root Task reuse and permanent Task mailbox logic.
- Delete TaskRegistry business state and internal lifecycle observer.
- Delete TaskControlPort and multi-agent tool providers from the default build.
- Remove TaskChanged/WorkChanged from the single-agent protocol path.

Exit criteria:

- hostd and TUI no longer depend on Task/Work lifecycle;
- workspace tests pass without the old runtime path.

### Phase 6: Storage and Documentation Cleanup

- Decide whether to retain read compatibility for legacy task shards.
- Stop writing new Task/Work lifecycle records.
- Update recovery, storage, API, and observation documentation.
- Remove documents that describe the old runtime as current.

Exit criteria:

- new sessions use only the Execution model;
- legacy behavior is either explicitly supported or explicitly rejected.

### Phase 7: First Multi-Agent Extension

Only after the single-agent invariants are stable:

- add `parent_execution_id` and `agent_spec_id`;
- implement one attached child Execution with a private transcript;
- return the child terminal report as a parent tool result;
- add an attached-child barrier to the parent finalizer;
- defer detached execution and long-lived Agent instances.

## 11. Verification Matrix

### Execution lifecycle

| Scenario | Required result |
|---|---|
| start Execution | Accepted → Running → one terminal outcome |
| concurrent start in one Session | rejected in single-agent mode |
| different Sessions | may execute concurrently |
| provider error | ExecutionFailed and TurnFailed |
| persistence failure | no uncommitted context use; deterministic failure |
| cancellation | ExecutionCancelled then TurnCancelled |
| runtime panic | finalizer produces infrastructure failure |
| duplicate terminal | idempotent if identical; conflict if different |

### Model Steps and tools

| Scenario | Required result |
|---|---|
| plain response | one Model Step |
| tool continuation | multiple Model Steps in one Execution |
| normal tool error | committed tool result; Execution may continue |
| steering | injected between Model Steps in same Execution |
| follow-up | later Turn and Execution |

### Persistence and observation

| Scenario | Required result |
|---|---|
| initial input commit fails | no provider request |
| MessageCommitted | published only after CommitAck and projection |
| delta lag | may drop without affecting recovery |
| subscriber disconnect | Execution continues |
| cursor expiration | snapshot and resubscribe |
| live committed lookup | HostState only, no hot JSONL fallback |

### Recovery

| Scenario | Required result |
|---|---|
| completed Turn | transcript and outcome restored |
| non-terminal historical Turn | interrupted/failed, never permanent Running |
| committed assistant without Execution terminal | recovery-required failure |
| legacy task shard | read compatibility follows explicit migration policy |

## 12. Main Risks

### Storage compatibility

Existing sessions encode Task and Work identity. The new API must not preserve
those concepts merely to avoid a storage migration. Use a compatibility adapter
or explicit legacy reader.

### TUI agent projections

Agent panel state currently depends on Task identity. Single-agent UI should
derive activity from the active Execution and Session projection. Multi-agent UI
must wait for the execution-tree design.

### Approval and interaction routing

Existing callbacks are globally rebound. The new path must carry immutable
session/execution identity through every request and response.

### Mixed runtime paths

Running old Task and new Execution paths simultaneously can duplicate Messages
or terminal outcomes. A Session must select exactly one runtime path.

## 13. Change-Scope Guidance

Prefer vertical slices over directory-wide renames:

1. add the new contract;
2. execute one prompt end to end;
3. move hostd root Turn traffic;
4. complete cancellation and recovery;
5. delete the old path;
6. rename retained Step/tool components after behavior is stable.

The migration is complete only when the old Task/Work lifecycle is no longer a
source of truth for single-agent execution.
