# Dispatcher Framework

> 历史设计：本文档中的 `DisplayEvent`/Display channel 已被移除，不再描述当前实现。当前契约见 `packages/orchd/docs/events-and-observation.md` 和 `docs/session-output-projection.md`。

dispatcher framework 定义 orchd runtime 如何接收输入、如何在内部做分流、以及如何把结果落到 hostd-facing typed channels。

本文档不是 message schema 手册，而是 runtime routing/framework 规范。类型全集见 `docs/message-types.md`；多 agent 心智模型见 `docs/multi-agent-mental-model.md`；stream 边界见 `docs/stream-architecture.md`。

---

## 1. Design Goals

- **输入分流**：runtime 必须显式区分 task start、steer、gateway stream、tool result、cancel、close 等不同输入面。
- **职责分层**：supervisor、task orchestrator、step dispatch、downstream consumers 各自只承担一个层级的职责。
- **typed sink**：display、persist、lifecycle 仍然是唯一的 hostd-facing typed channels。
- **本地一致性**：`senders=None` 只表示 local collecting sink，不表示可以静默丢弃事件。
- **多 agent 可扩展**：父 task、子 task、detached task 共用统一 dispatch contract，由 `task_id` 区分。

---

## 2. Framework Scope

dispatcher framework 覆盖 4 层对象：

1. `Supervisor / Runtime Registry`
2. `Task / Turn Orchestrator`
3. `Step Dispatch`
4. `Downstream Consumers / Sinks`

它们的关系如下：

```text
Supervisor
  └─ owns task registry / cross-task control
      └─ Task Orchestrator
           └─ launches Step Dispatch
                └─ produces routed results
                     └─ sinks to transcript / display / persist / lifecycle / tool runtime
```

---

## 3. Core Primitives

### 3.1 SessionChannels

`SessionChannels` 是 session 级 typed channel bundle。

```rust
pub struct SessionChannels {
    persist_tx: mpsc::Sender<Arc<PersistEvent>>,
    persist_rx: Option<mpsc::Receiver<Arc<PersistEvent>>>,
    display_tx: mpsc::Sender<Arc<DisplayEvent>>,
    display_rx: Option<mpsc::Receiver<Arc<DisplayEvent>>>,
    lifecycle_input_tx: mpsc::UnboundedSender<LifecycleEvent>,
    lifecycle_input_rx: Option<mpsc::UnboundedReceiver<LifecycleEvent>>,
    lifecycle_tx: mpsc::Sender<Arc<LifecycleEvent>>,
    lifecycle_rx: Option<mpsc::Receiver<Arc<LifecycleEvent>>>,
}
```

一个 session 内所有 task runtime 共用一组 `SessionChannels` 做 fan-in。typed event 的 demux 依赖事件自带的 `task_id` 与 `agent_id`。

### 3.2 DispatchSenders

`DispatchSenders` 是 runtime sink 持有的 sender clone。

```rust
pub struct DispatchSenders {
    pub persist: mpsc::Sender<Arc<PersistEvent>>,
    pub display: mpsc::Sender<Arc<DisplayEvent>>,
    pub lifecycle: mpsc::UnboundedSender<LifecycleEvent>,
}
```

约束：

- persist sender 必须以 awaited send 使用。
- lifecycle sender 对 terminal / close 相关事件必须可靠。
- display sender 承载高频 delta，但不得替代 persist 事实。

### 3.3 Local Collecting Sink

当 `senders=None` 时，runtime 不直接发 channel，而是落到 local collecting sink。

作用：

- 收集 `DisplayEvent`
- 收集 `PersistEvent`
- 将 task lifecycle 以 `ServerMessage::TaskLifecycle` 和对应的
  `PersistEvent::TaskEventCommitted` 收入 local event stream
- 让本地测试和生产共享同一批分流结果

约束：

- collecting sink 与 typed channels 的差别只能是“落地点不同”。
- 不能因为 `senders=None` 就丢掉 `ToolStarted`、`ToolEnded`、`ToolResultCommitted` 或 lifecycle 事实。

---

## 4. Runtime Input Surfaces

dispatcher framework 处理的不是单一 gateway stream，而是以下 6 类输入面：

| 输入面 | 第一接收层 | 示例 |
|---|---|---|
| `TaskStart` | supervisor -> task orchestrator | root task start, spawned task start |
| `SteerInput` | supervisor -> task orchestrator | user follow-up, `steer_task` |
| `GatewayEvent` | step dispatch | `ContentDelta`, `ToolCallChunk`, `Done` |
| `ToolExecutionResult` | tool runtime consumer | regular tool result, spawn report |
| `Cancellation` | supervisor + orchestrator + tool runtime | turn cancel, shutdown |
| `LifecycleControl` | supervisor + orchestrator | close, reopen, driver teardown |

结论：

- gateway stream 只是 dispatcher framework 的一个输入面。
- 如果只建模 gateway consumer，task start、steer、spawn、close 的职责一定会回流到 `agent_loop`。

---

## 5. Runtime Layers

### 5.1 Supervisor / Runtime Registry

supervisor 是 orchd 全局控制平面，不属于某个单独 task。

职责：

- 处理 `spawn` / `spawn_detached` / `poll_task` / `steer_task`
- 分配 `task_id`
- 维护 `task_id -> handle` 注册表
- 以 `(session_id, agent_id)` 隔离 root task 复用
- 启动 task driver
- 保存 task result cache
- 处理 close / cancel / cleanup

不负责：

- transcript merge
- 单次 LLM step dispatch
- hostd persistence

### 5.2 Task / Turn Orchestrator

task orchestrator 管理单个 task 的长生存状态机。

职责：

- 消费 `TaskStart`、`SteerInput`、`Cancellation`、`LifecycleControl`
- 维护 task running / idle / closed 等状态
- 维护 in-memory transcript ownership
- 决定何时启动一个新的 model step

### 5.3 Step Dispatch

step dispatch 只处理一次 model step。

职责：

- 消费 `GatewayEvent`
- 聚合 assistant text / thinking / usage
- 聚合 tool call chunks
- 生成 `StepDispatchResult`

tool runtime 不在 step dispatch hook 内执行。task orchestrator 收到完整
`StepDispatchResult` 后，在 assistant commit 与工具执行之间检查取消状态，再调用
`ToolExecutionConsumer`。

step dispatch 不是全 task 生命周期控制器。

### 5.4 Downstream Consumers / Sinks

下游 consumer 消费 orchestrator/step dispatch 产出的专用结果对象。

标准 consumer：

- transcript consumer
- display consumer
- persist consumer
- lifecycle consumer
- tool runtime consumer

---

## 6. Routed Results

dispatcher framework 的核心不是“谁直接发 channel”，而是“先按职责分流，再由对应 sink 落地”。

这不要求代码里定义一个统一的 union type。推荐做法是各层返回自己的专用结果对象，例如：

- transcript / display / persist / lifecycle 的职责边界清楚
- step dispatch 返回 `StepDispatchResult`
- tool runtime 返回 `ToolExecutionResult`
- lifecycle sink 接收 `TaskEvent`
- 本地模式和生产模式共享同一批分流结果，只是 sink 不同

---

## 7. Channel Contracts

### 7.1 Persist Channel

persist channel 承载 committed / recoverable state。

```rust
pub enum PersistEvent {
    Finalized { session_id, message_id, task_id, agent_id, message },
    ToolCallCommitted { session_id, message_id, task_id, agent_id, parent_message_id, message },
    ToolResultCommitted { session_id, message_id, task_id, agent_id, message },
    TaskEventCommitted(TaskEvent),
}
```

consumer：

- hostd storage
- session state
- resume source of truth

规则：

- 不允许中间态 delta 混入 persist。
- 不允许静默丢弃。
- `TaskEventCommitted` 进入持久化 metadata，但不是 transcript message。

### 7.2 Display Channel

display channel 承载 live rendering。

```rust
pub enum DisplayEvent {
    MessageStart { ... },
    TextDelta { ... },
    ThinkingDelta { ... },
    ToolCallDelta { ... },
    MessageEnd { ... },
    Finalized { ... },
    ToolStarted { ... },
    ToolEnded { ... },
    InteractionRequested { ... },
    InteractionResolved { ... },
}
```

consumer：

- hostd live view projection
- TUI timeline

规则：

- display event 不承担恢复语义。
- 所有 display event 都必须携带 `task_id` 和 `agent_id`。

### 7.3 Lifecycle Channel

lifecycle channel 承载 task / turn 编排状态。

```rust
pub enum LifecycleEvent {
    Task(TaskEvent),
    Turn(TurnEvent),
}
```

consumer：

- hostd task DAG projection
- turn status projection
- TUI task/agent status

规则：

- `TaskEvent` 由 orchd 产出。
- `TurnEvent` 由 hostd 产出或确认。
- lifecycle 到 persist 的镜像规则必须集中实现，不能在多个 helper 里重复发送。

---

## 8. Step Routing Contract

step dispatch 对单次 `GatewayEvent` 流的标准处理顺序如下：

| GatewayEvent | Step action | Routed result |
|---|---|---|
| `ContentDelta` | append text buffer | `Display(TextDelta)` |
| `ReasoningDelta` | append thinking buffer | `Display(ThinkingDelta)` |
| `ToolCallChunk` | aggregate into tool call item | `Display(ToolCallDelta)` |
| `Usage` | store step usage | attached to finalized assistant |
| `Done` | finalize assistant message | `Persist(Finalized)` + `Display(Finalized)` |
| `Error` | finalize error assistant or fail task | display final state + lifecycle failure |

标准 step 完成顺序：

```text
MessageStart
TextDelta*
ThinkingDelta*
ToolCallDelta*
MessageEnd
PersistEvent::Finalized
DisplayEvent::Finalized
PersistEvent::ToolCallCommitted*
ExecuteTools(tool_calls)
```

约束：

- `PersistEvent::Finalized` 必须先于同一步的 `ToolCallCommitted`。
- tool call commit 必须先于对应 result commit。
- assistant finalize 与工具执行之间必须保留可见的取消边界。

---

## 9. Tool Runtime Routing

tool runtime consumer 接收的是完整 tool call，而不是 `GatewayEvent`。

对每个 tool call：

1. 产生 `DisplayEvent::ToolStarted`
2. 执行 regular tool 或 spawn tool
3. 产生 `DisplayEvent::ToolEnded`
4. 构建 `Message::ToolResult`
5. 可靠地产生 `PersistEvent::ToolResultCommitted`
6. 产生 transcript delta 供下一 step 使用

### Spawn Tools

spawn 类工具额外经过 supervisor：

```text
parent task
  -> spawn request
  -> supervisor registry
  -> child task runtime
  -> child report
  -> parent tool result
```

约束：

- 父 task 发起的是请求，不是直接实例化 child loop。
- child task 的 typed events 与父 task 的 typed events共用同一个 session fan-in。

---

## 10. Hostd Event Bus

hostd 消费：

- `persist_rx`
- `display_rx`
- `lifecycle_rx`
- approval / interaction side events
- queue / model / auth / command events

hostd 负责：

- 将 persist event 写入 storage/state
- 将 lifecycle event 投影为 task DAG / turn status
- 将 display event 物化为 per-task live view
- 将 display/lifecycle/approval/queue/model/auth 包装为 `ServerMessage`
- 构建 session snapshot 和 task view snapshot

`ServerMessage` 是 TUI 唯一可见的线协议；supervisor registry 不直接暴露给 TUI。

---

## 11. Multi-Agent Fan-In

一个 session 内的所有 task runtime 共用一组 `SessionChannels`：

```text
root task runtime   ─┐
child task runtime  ─┼─> SessionChannels ─> hostd event bus
detached task       ─┘
```

spawn 请求路径则额外经过 supervisor：

```text
parent task runtime
  └─ spawn request
       └─ supervisor registry
            └─ child task runtime
```

这套 fan-in 成立的前提是所有 typed event 都携带：

- `session_id`（需要持久化时）
- `task_id`（runtime identity）
- `agent_id`（template identity）
- `parent_task_id` / `source_agent_id`（create/steer 边界）

hostd 负责按 `task_id` demux 成 per-task live view；orchd 不向 TUI 直接暴露 per-agent wire stream。

---

## 12. Backpressure and Failure

channel send policy 是 framework contract 的一部分：

| Channel / Sink | Delivery policy | Failure handling |
|---|---|---|
| persist | reliable | fail turn / surface runtime error |
| lifecycle | reliable for terminal and close semantics | fail or surface inconsistency explicitly |
| display | reliable to hostd live view store | may coalesce/materialize, never changes transcript truth |
| local collecting sink | reliable inside current process | return collected routed results to caller |
| supervisor registry | reliable for active task handles | explicit not-found / closed / cancelled result |

规则：

- persist 和 terminal lifecycle 禁止 silent loss。
- `senders=None` 不是 silent loss 的借口。
- close / cancel / spawn failure 必须以显式结果或 lifecycle 状态表现出来。
