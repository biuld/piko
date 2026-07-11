# Public API

> Status: current  
> Audience: both

The agent runtime contract lives in the **`orchd-api`** crate. DTOs are defined in `piko-protocol`; the default implementation is `orchd::AgentRuntimeService`.

## Four planes

```text
Command API       create_task / submit_input / control_task
Snapshot API      session_snapshot / task_snapshot
Observation API   subscribe_session
Integration Port  PersistSink request/ack  (implemented by hostd)
```

The first three are the caller-facing Agent API. `PersistSink` is the integration contract. Observation is a formal output contract, not peripheral UI plumbing.

## Crate layout

**`orchd-api`** (integrator dependency):

```rust
// traits, errors, ports, helpers
use orchd_api::{
    AgentRuntime, PersistSink, ToolProvider, ApprovalGateway,
    build_user_input, AgentApiError, SessionSubscription, …
};
```

**`orchd`** (implementation + bootstrap):

```rust
pub mod api;        // re-exports orchd-api + AgentRuntimeService
pub mod bootstrap;  // Runtime::bootstrap
pub mod tools;      // UserInteractionProvider for host bridges
pub use orchd_api;
pub use bootstrap::Runtime;
```

Integrators should depend on **`orchd-api`** for all traits and port types. Link **`orchd`** only to construct and run the runtime.

hostd must **not** depend on internal orchd modules (`application`, `runtime`, `domain`, `ports`, `adapters`).

## AgentRuntime trait

```rust
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    async fn create_task(&self, request: CreateTaskRequest) -> Result<TaskHandle, AgentApiError>;
    async fn submit_input(&self, request: SubmitTaskInput) -> Result<InputReceipt, AgentApiError>;
    async fn control_task(&self, request: TaskControlRequest) -> Result<TaskSnapshot, AgentApiError>;
    async fn task_snapshot(&self, task_id: String) -> Result<TaskSnapshot, AgentApiError>;
    async fn session_snapshot(&self, session_id: String) -> Result<SessionRuntimeSnapshot, AgentApiError>;
    async fn subscribe_session(&self, request: SubscribeRequest) -> Result<SessionSubscription, AgentApiError>;
}
```

Implementation: `AgentRuntimeService` (internal supervisor-backed facade).

Helpers not on the trait:

- `AgentRuntimeService::start_root_turn(...)` — hostd turn bootstrap (subscribe + create/reuse root + first input)
- `Runtime::agent_runtime()` — obtain the service from a bootstrapped runtime

## Command: create_task

```rust
pub struct CreateTaskRequest {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub task_id: Option<TaskId>,
    pub agent_id: AgentId,
    pub parent_task_id: Option<TaskId>,
    pub source: InputSource,
    pub mode: TaskMode,
    pub host_context: HostTaskContext,
    pub resume: Option<TaskResumeState>,
}
```

- **Does not carry a prompt.** Creation and first input are separate operations.
- `resume` is only for hostd rebuilding a runtime from a task shard; normal creation must pass `None`. Recovery does not re-emit `TaskCreated` or replay historical user input.

Returns `TaskHandle { session_id, task_id, agent_id, status }`.

## Command: submit_input

**The only entry point for user-role transcript input.**

```rust
pub struct SubmitTaskInput {
    pub request_id: RequestId,
    pub session_id: SessionId,
    pub task_id: TaskId,
    pub message_id: MessageId,
    pub work_id: WorkId,
    pub source_turn_id: Option<TurnId>,
    pub source: InputSource,
    pub content: MessageContent,
    pub delivery: InputDelivery,
    pub submitted_at: i64,
}
```

```rust
pub enum InputSource {
    User,
    Task { task_id, agent_id },
    System { component: String },
}
```

Source is provenance only; it does not change message role. A parent task's initial prompt and steer both appear as `Message::User` in the child transcript.

```rust
pub enum InputDelivery {
    Immediate,
    AfterCurrentStep,
}
```

```rust
pub struct InputReceipt {
    pub request_id, pub task_id, pub work_id, pub message_id,
    pub disposition: InputDisposition,  // Accepted | Queued | Duplicate
}
```

hostd can use `orchd_api::build_user_input(...)` to allocate `request_id`, `message_id`, and default `AfterCurrentStep`.

## Command: control_task

```rust
pub enum TaskControlRequest {
    Close { request_id, task_id },
    Reopen { request_id, task_id },
    CancelWork { request_id, task_id, work_id },
    Terminate { request_id, task_id },
}
```

- `CancelWork` — aborts the current work; the task can still accept input.
- `Close` — rejects new input; allows reopen.
- `Terminate` — ends the runtime handle.

## Snapshot

Streams cannot replace snapshots: subscriptions may disconnect, deltas may be dropped, and reliable events have retention limits.

- `task_snapshot(task_id)` — live state of one task
- `session_snapshot(session_id)` — task DAG projection for the session + cursor

Durable transcript content still comes from hostd `TaskRepository`; orchd snapshots do not replace durable recovery.

## Observation: subscribe_session

```rust
pub struct SubscribeRequest {
    pub session_id: SessionId,
    pub task_id: Option<TaskId>,      // optional filter
    pub after: Option<SessionCursor>,
}

pub struct SessionSubscription {
    pub session_id: SessionId,
    pub cursor: SessionCursor,
    pub output: SessionOutputStream,
}
```

**Rules:**

- Subscription scope is the **session** (dynamically spawned child tasks appear on the same subscription).
- `create_task` / `submit_input` do **not** return per-task streams.
- Recommended reconnect: `session_snapshot` → record cursor → `subscribe_session(after = cursor)`.

### SessionOutput

```rust
pub enum SessionOutput {
    Event(SessionEventEnvelope),   // reliable
    Delta(RealtimeDeltaEnvelope),  // realtime, droppable
}
```

| Lane | Guarantees |
|---|---|
| **Event** | Published only after durable commit succeeds; ordered by `task_seq`; includes `TaskChanged`, `MessageCommitted`, `ToolCommitted` |
| **Delta** | Not persisted; not used for recovery; may be dropped under lag; `delta_seq` orders deltas within one message only |

The two lanes have **no global ordering guarantee**. Clients treat `MessageCommitted` as authoritative and use it to correct temporary deltas.

### Stream errors

`SessionStreamError` (observation) and `AgentApiError` (command) must **not** be conflated.

When the cursor cannot be renewed, the stream yields `SnapshotRequired` and ends; the client fetches a new snapshot and resubscribes.

## Integration: PersistSink

Implemented by hostd; orchd calls it at the commit barrier:

```rust
#[async_trait]
pub trait PersistSink: Send + Sync {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError>;
    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError>;
    async fn commit_work_event(&self, event: WorkEventCommit) -> Result<PersistAck, PersistError>;
}
```

Details: [persistence.md](persistence.md).

## orchd-api modules

| Module | Content |
|---|---|
| `runtime` | `AgentRuntime` trait |
| `stream` | `SessionSubscription`, `SessionOutputStream` |
| `persist` | `PersistSink`, `MessageCommit`, `PersistAck`, … |
| `tools` | `ToolProvider`, `ToolExecResult`, discovery/execution contexts |
| `approval` | `ApprovalGateway`, approval request/decision types |
| `input` | `build_user_input()` |
| `error` | `AgentApiError`, `SessionStreamError` |
| `request` / `response` | Re-exports from `piko-protocol` |

See also [`orchd-api` README](../../orchd-api/README.md).

## orchd::Runtime (bootstrap)

Public bootstrap surface for constructing an in-process runtime:

| Method | Purpose |
|---|---|
| `Runtime::bootstrap` | Initialize model, built-in tools, task control |
| `register_agent` | Register `AgentSpec` |
| `set_persist_sink` | Inject session-level `PersistSink` |
| `register_tool_provider` / `register_tool_set` | MCP and host tool wiring |
| `set_approval_gateway` | Tool approval bridge |
| `agent_runtime()` | Obtain `AgentRuntimeService` for commands |

Port traits (`ToolProvider`, `ApprovalGateway`, `build_user_input`) live in **`orchd-api`**. User-interaction tools live in **`orchd::tools`**.

**hostd production code should issue commands only through `AgentRuntime`.** Bootstrap and tool wiring go through `orchd::Runtime`; port traits come from **`orchd-api`**. Internal types (`Supervisor`, `SessionOutputHub`, …) are not part of the integrator contract — orchd integration tests may access them via `orchd::testing` only.

## Related reading

- [host-integration.md](host-integration.md) — how hostd calls this API
- [events-and-observation.md](events-and-observation.md) — Event/Delta details
