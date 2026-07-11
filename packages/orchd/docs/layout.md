## 13. orchd Directory Design

### 13.1 Principles

顶层采用稳定分层，runtime 内按执行阶段聚合：

```text
api          orchd 对调用方的统一入口
application  create/submit/control/observe 用例编排
domain       纯业务对象和状态规则
runtime      单个 task 的实际执行链
ports        orchd 依赖的外部能力接口
adapters     ports 的具体适配
```

依赖约束：

```text
api → application
application → domain + runtime + ports
runtime → domain + ports
adapters → ports + external crates
domain must not depend on application/runtime/adapters
```

protocol DTO 保持在 `piko-protocol`。orchd 不在 protocol crate 中放 runtime trait、execution context 或 side effects。

### 13.2 Target Tree

```text
packages/orchd/src/
├── lib.rs
├── api/
│   ├── mod.rs
│   ├── runtime.rs
│   ├── request.rs
│   ├── response.rs
│   ├── error.rs
│   └── stream.rs
├── application/
│   ├── mod.rs
│   ├── service.rs
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── create_task.rs
│   │   ├── submit_input.rs
│   │   └── control_task.rs
│   ├── queries/
│   │   ├── mod.rs
│   │   ├── task_snapshot.rs
│   │   ├── list_tasks.rs
│   │   └── poll_work.rs
│   └── supervision/
│       ├── mod.rs
│       ├── supervisor.rs
│       ├── registry.rs
│       ├── handle.rs
│       ├── launcher.rs
│       └── driver.rs
├── domain/
│   ├── mod.rs
│   ├── agents/
│   │   ├── mod.rs
│   │   └── spec.rs
│   ├── tasks/
│   │   ├── mod.rs
│   │   ├── identity.rs
│   │   ├── task.rs
│   │   ├── state.rs
│   │   ├── lifecycle.rs
│   │   ├── input.rs
│   │   └── control.rs
│   ├── work/
│   │   ├── mod.rs
│   │   ├── work.rs
│   │   ├── state.rs
│   │   └── result.rs
│   ├── transcript/
│   │   ├── mod.rs
│   │   ├── transcript.rs
│   │   └── committed_message.rs
│   ├── model/
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   ├── step.rs
│   │   └── usage.rs
│   └── tools/
│       ├── mod.rs
│       ├── definition.rs
│       ├── call.rs
│       ├── result.rs
│       ├── policy.rs
│       └── approval.rs
├── runtime/
│   ├── mod.rs
│   ├── task/
│   │   ├── mod.rs
│   │   ├── orchestrator.rs
│   │   ├── context.rs
│   │   ├── state.rs
│   │   ├── mailbox.rs
│   │   ├── input.rs
│   │   ├── flow.rs
│   │   └── recovery.rs
│   ├── step/
│   │   ├── mod.rs
│   │   ├── runner.rs
│   │   ├── source.rs
│   │   ├── assembly.rs
│   │   ├── stream.rs
│   │   └── output.rs
│   ├── tools/
│   │   ├── mod.rs
│   │   ├── executor.rs
│   │   ├── parallel.rs
│   │   └── sequential.rs
│   └── events/
│       ├── mod.rs
│       ├── emitter.rs
│       ├── hub.rs
│       ├── output.rs
│       ├── event_lane.rs
│       ├── delta_lane.rs
│       ├── collector.rs
│       └── internal_lifecycle.rs
├── ports/
│   ├── mod.rs
│   ├── model_gateway.rs
│   ├── persist_sink.rs
│   ├── task_control.rs
│   ├── tool_provider.rs
│   ├── approval_gateway.rs
│   ├── clock.rs
│   └── id_generator.rs
└── adapters/
    ├── mod.rs
    ├── model/
    │   └── mod.rs
    └── tools/
        ├── mod.rs
        ├── registry.rs
        ├── task_control.rs
        ├── workspace.rs
        ├── todo.rs
        └── user_interaction.rs
```

---

## 14. Directory Responsibilities

### 14.1 `api/`

`api` 是 orchd 唯一公开面，只描述如何调用 orchd：

- `runtime.rs`: `AgentRuntime` trait/facade。
- `request.rs`: create/input/control request re-exports 或 orchd-local wrappers。
- `response.rs`: handle、receipt、snapshot。
- `error.rs`: stable API error。
- `stream.rs`: subscription/event stream public types。

`lib.rs` 应最终收紧为：

```rust
mod adapters;
mod application;
mod domain;
mod ports;
mod runtime;

pub mod api;

pub use api::{AgentApiError, AgentRuntime, AgentRuntimeService};
```

hostd 不应直接访问 `TaskRunState`、`TaskRegistry`、`StepDispatch` 或 tool consumer。

### 14.2 `application/`

application 负责编排用例，不执行 transcript mutation。

`service.rs` 实现 Agent API facade；`commands` 处理写操作；`queries` 处理只读操作；`supervision` 管理 runtime handle。

`submit_input` command 的职责：

```text
validate API request
  → locate task handle
  → send TaskMailboxMessage::Input
  → await InputReceipt
```

它不得直接 push transcript、emit `UserCommitted`、调用 LLM 或写 JSONL。

### 14.3 `application/supervision/`

- `supervisor.rs`: 分配 task、协调 launcher、处理全局 task operation。
- `registry.rs`: `task_id → TaskHandle/metadata`。
- `handle.rs`: mailbox sender、cancel token、join handle 等。
- `launcher.rs`: 构造并启动一个 task runtime。
- `driver.rs`: 驱动 runtime stream、记录退出结果和清理 handle。

Supervisor 不拥有 transcript，不解释 input 内容，不承担 durable storage。

### 14.4 `domain/`

domain 只包含纯对象和状态规则：

- AgentSpec 静态能力定义；
- task identity/state/lifecycle；
- work state/result；
- transcript append 不变量；
- model step value objects；
- tool call/result/policy。

当前 `domain/model/transcript.rs` 应移动到 `domain/transcript`。`AgentTask` 不应继续同时表示创建请求、initial prompt、runtime state 与 recovery payload。

建议拆成：

```text
AgentTask             task identity and metadata
SubmitTaskInput       input command
TaskRunState          live runtime state
RecoveredTask         recovery payload
```

### 14.5 `runtime/task/`

这里执行一个具体 task。现有 `runtime/orchestrator` 应重命名为 `runtime/task`，避免与全局 orchd/supervisor 混淆。

```rust
pub(crate) struct TaskRuntime {
    context: TaskContext,
    state: TaskRunState,
    mailbox: TaskMailbox,
    step_runner: StepRunner,
    emitter: TaskEventEmitter,
}
```

主循环应表现为状态机：

```rust
loop {
    match self.next_action().await? {
        TaskAction::CommitInput(input) => self.commit_input(input).await?,
        TaskAction::RunStep => self.run_step().await?,
        TaskAction::ExecuteTools(calls) => self.execute_tools(calls).await?,
        TaskAction::EnterIdle => self.enter_idle().await?,
        TaskAction::ApplyControl(control) => self.apply_control(control).await?,
        TaskAction::Stop => break,
    }
}
```

`state.rs` 只保存业务状态，不保存 channel sender、storage path 或 global registry。channel 与 collector 属于 emitter/sink。

### 14.6 `runtime/step/`

只负责一次 model step：

```text
transcript snapshot
  → model gateway
  → consume GatewayEvent
  → assemble assistant/tool calls/usage
  → return StepOutput
```

```rust
pub struct StepOutput {
    pub assistant: AssistantCandidate,
    pub tool_calls: Vec<ToolCallCandidate>,
    pub display_events: Vec<DisplayEvent>,
    pub usage: Option<Usage>,
    pub stop_reason: Option<String>,
}
```

step runner 不直接写 JSONL，也不修改 supervisor registry。

### 14.7 `runtime/events/`

现有 `runtime/dispatch` 同时承担 gateway consumer、channel bus、persist、display、lifecycle、collector 和 tool routing，含义过载。目标拆为 `step`、`events` 和 `tools`。

统一 emitter：

```rust
pub(crate) struct TaskEventEmitter {
    durable: Arc<dyn PersistSink>,
    output: Arc<SessionOutputHub>,
    lifecycle: Arc<dyn InternalLifecycleObserver>,
}
```

业务逻辑只调用：

```text
commit_message
commit_task_event
publish_event
publish_delta
notify_internal_lifecycle
```

`SessionOutputHub` 是 session-scoped，不是 turn-scoped。root 和所有 child task 根据 `session_id` 使用同一个 hub，不通过 spawn/steer 传递 `DispatchSenders`：

```rust
pub(crate) struct SessionOutputHub {
    reliable_events: Arc<dyn EventSink<SessionEventEnvelope>>,
    realtime_deltas: Arc<dyn EventSink<RealtimeDeltaEnvelope>>,
}
```

对外订阅：

```rust
pub struct SessionSubscription {
    pub output: SessionOutputStream,
}
```

生产 channel 与本地 collector 是 lane/sink implementation，不是两套 consumer：

```rust
pub trait EventSink<T> {
    async fn send(&self, event: T) -> Result<(), SendError>;
}
```

可提供：

```text
ChannelEventSink
CollectingEventSink
FanoutEventSink
```

这样不再存在 `senders=None`，也不需要在 assistant、tool 和 lifecycle consumer 中分别实现 fallback。

原有 `SessionChannels` 如果保留名字，只能作为 subscription transport；更准确的名字是 `SessionSubscription`。它不得再包含 persist stream、lifecycle input、dispatch launcher 或可向 task 传播的 sender bundle。

### 14.8 `ports/`

ports 是 orchd 对外部能力的依赖：

- `model_gateway`: LLM provider gateway。
- `persist_sink`: durable host persistence barrier。
- `task_control`: 提供给 task-control tool 的受限 capability。
- `tool_provider`: tool catalog/provider。
- `approval_gateway`: interaction/approval。
- `clock`: deterministic timestamp。
- `id_generator`: deterministic request/task/work/message IDs。

当前 `AgentSpawner` 同时包含 spawn、steer、poll 等大量 AgentRuntime 能力，应逐步替换为 public `AgentRuntime` 和 capability-limited `TaskControlPort`。

### 14.9 `adapters/`

adapters 实现 ports，不包含 task state machine。

task control tool adapter 只能持有受限 `TaskControlPort`，不能直接访问 SupervisorState 或 TaskRegistry。

---

## 15. Current-to-Target File Mapping

| Current | Target |
|---|---|
| `application/supervisor.rs` | `application/service.rs` + `application/supervision/supervisor.rs` |
| `application/task_registry.rs` | `application/supervision/registry.rs` |
| `application/task_launcher.rs` | `application/supervision/launcher.rs` |
| `application/task_driver.rs` | `application/supervision/driver.rs` |
| `application/run.rs` | `commands/create_task.rs`、`commands/submit_input.rs` 和临时 forwarding facade |
| `application/agent_spawner.rs` | command handlers；最终删除 |
| `ports/agent_spawner.rs` | public `AgentRuntime` + restricted `TaskControlPort` |
| `runtime/agent_loop.rs` | `runtime/task/orchestrator.rs` entry |
| `runtime/orchestrator/*` | `runtime/task/*` |
| `runtime/types.rs` | `domain/tasks/input.rs`、`control.rs`、`runtime/task/mailbox.rs` |
| `runtime/dispatch/step/*` | `runtime/step/*` |
| `runtime/dispatch/consumer/display.rs` | `runtime/events/delta_lane.rs` + committed output projection |
| `runtime/dispatch/consumer/persist.rs` | `ports/persist_sink.rs` + task commit pipeline |
| `runtime/dispatch/consumer/lifecycle.rs` | durable lifecycle commit + `runtime/events/internal_lifecycle.rs` |
| `runtime/dispatch/bus.rs` | `runtime/events/hub.rs` + `runtime/events/output.rs` |
| `runtime/tool_executor/*` | `runtime/tools/*` |
| `domain/model/transcript.rs` | `domain/transcript/transcript.rs` |
| `domain/events/event.rs` | remove shallow re-export; use protocol type at boundary |
| `protocol/mod.rs` | `api/stream.rs` or `runtime/events/wire.rs` if conversion remains necessary |

---

## 16. Main Execution Paths

### 16.1 Submit Input

```text
api::AgentRuntime::submit_input
  → application::commands::submit_input
  → application::supervision::registry
  → runtime::task::mailbox
  → runtime::task::input::commit_input
  → ports::PersistSink
  → domain::Transcript::append
  → runtime::task::orchestrator
  → runtime::step::runner
```

### 16.2 Spawn Child

```text
task-control tool adapter
  → ports::TaskControlPort::create_child
  → application::commands::create_task
  → supervision::launcher
  → child TaskRuntime
  → application::commands::submit_input
  → child commit_input
```

### 16.3 Observe and Persist

```text
TaskRuntime
  → TaskEventEmitter
      ├─ PersistSink → hostd task JSONL + session manifest + HostState
      ├─ InternalLifecycleObserver → supervisor runtime projection
      └─ SessionOutputHub
          ├─ reliable event lane → hostd/TUI notification
          └─ realtime delta lane → hostd/TUI live rendering
```

---

## 17. Public Visibility

当前 orchd 不应长期公开所有内部 module。目标 public surface：

```rust
pub mod api;

pub use api::{
    AgentApiError,
    AgentRuntime,
    AgentRuntimeService,
    SessionOutputStream,
    SessionSubscription,
};
```

需要 hostd 实现的 integration ports 可以单独公开：

```rust
pub mod integration {
    pub use crate::ports::persist_sink::{
        PersistAck,
        PersistError,
        PersistSink,
    };
}
```

禁止其他 crate 依赖 orchd internal runtime types 形成旁路 API。

---

