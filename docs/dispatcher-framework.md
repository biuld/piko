# Dispatch Framework

dispatch framework 是 orchd 与 hostd 之间的 typed event boundary。它把 agent runtime 产生的事件按语义拆分为 persist、display、lifecycle，并交给 hostd event bus 消费。

本文档是架构规范。Agent identity 的全局定义见 `docs/agent-identity.md`；dispatch events 必须同时携带 runtime identity (`task_id`) 和 template identity (`agent_id`)。

---

## 1. Design Goals

- **类型窄化**：不同 consumer 只接收自己负责的事件类型。
- **可靠落盘**：persist channel 是强可靠路径，任何 transcript/session 恢复所需事件都必须到达 hostd。
- **渲染解耦**：display channel 承载高频流式渲染事件，但不能成为 transcript 事实来源。
- **编排独立**：task/turn lifecycle 不混入 display。
- **hostd 权威**：hostd 合并 typed channels 和 host-managed events，并对外输出统一 `ServerMessage`。

---

## 2. Core Primitives

### SessionChannels

`SessionChannels` 是 session 级 typed channel bundle。

```rust
pub struct SessionChannels {
    persist_tx: mpsc::Sender<Arc<PersistEvent>>,
    persist_rx: mpsc::Receiver<Arc<PersistEvent>>,
    display_tx: mpsc::Sender<Arc<DisplayEvent>>,
    display_rx: mpsc::Receiver<Arc<DisplayEvent>>,
    lifecycle_tx: mpsc::Sender<Arc<LifecycleEvent>>,
    lifecycle_rx: mpsc::Receiver<Arc<LifecycleEvent>>,
}
```

每个 session 使用一组 `SessionChannels` 作为统一 fan-in。该 session 内所有 agent task instance 都投递到这组 channels，事件必须携带 `task_id` 和 `agent_id`。

### DispatchSenders

`DispatchSenders` 是 agent runtime 和 tool runtime 持有的 sender clone。

```rust
pub struct DispatchSenders {
    pub persist: mpsc::Sender<Arc<PersistEvent>>,
    pub display: mpsc::Sender<Arc<DisplayEvent>>,
    pub lifecycle: mpsc::Sender<Arc<LifecycleEvent>>,
}
```

约束：

- persist sender 必须以 awaited send 方式使用。
- lifecycle sender 应保证 terminal task events 可达。
- display sender 承载高频 delta，hostd 必须消费并物化到 per-task live view store。

---

## 3. Event Channels

### 3.1 Persist Channel

persist channel 承载最终态和可恢复状态。

```rust
pub enum PersistEvent {
    Finalized { session_id, message_id, task_id, agent_id, message },
    ToolCallCommitted { session_id, message_id, task_id, agent_id, parent_message_id, message },
    ToolResultCommitted { session_id, message_id, task_id, agent_id, message },
    TaskLifecycle(TaskEvent),
}
```

consumer：hostd storage/session state。

规则：

- 不允许发送中间态 delta。
- 不允许静默丢弃。
- hostd 负责把 message persist events 转换为 `SessionTreeEntry`。
- task lifecycle 落为 task DAG metadata，不写成 transcript message。

### 3.2 Display Channel

display channel 承载 TUI live rendering。

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

consumer：hostd event bus → TUI timeline。

规则：

- display event 不写 JSONL。
- `Finalized` 是 display 的最终渲染事件，不替代 `PersistEvent::Finalized`。
- 所有 display event 必须包含 `task_id` 和 `agent_id`。

### 3.3 Lifecycle Channel

lifecycle channel 承载编排状态。

```rust
pub enum LifecycleEvent {
    Task(TaskEvent),
    Turn(TurnEvent),
}
```

consumer：hostd task/turn state → TUI status/agent list。

规则：

- `TaskEvent` 由 orchd 产生。
- `TurnEvent` 由 hostd 产生或确认。
- task terminal events 必须和 persist completion 一起被 hostd 消费，不能提前宣布 turn 完成。

---

## 4. Agent Runtime Routing

`agent_loop` 是唯一规范的 LLM event consumer。它从 llmd 接收 `GatewayEvent` 并按下表路由：

| GatewayEvent | Runtime action | Output |
|---|---|---|
| `ContentDelta` | append text buffer | `DisplayEvent::TextDelta` |
| `ReasoningDelta` | append thinking buffer | `DisplayEvent::ThinkingDelta` |
| `ToolCallChunk` | aggregate into complete tool call | none until finalized |
| `Usage` | store step usage | attached to final assistant message |
| `Done` | finalize assistant message | `PersistEvent::Finalized` + `DisplayEvent::Finalized` |
| `Error` | finalize with error or fail task | display final state + task failure |

Step finalization sequence:

```rust
MessageStart
TextDelta*
ThinkingDelta*
MessageEnd
PersistEvent::Finalized
DisplayEvent::Finalized
PersistEvent::ToolCallCommitted*
ToolStarted*
ToolEnded*
PersistEvent::ToolResultCommitted*
TaskEvent::Completed | TaskEvent::Failed | next step
```

Ordering constraints:

- `PersistEvent::Finalized` must be sent before the corresponding tool call commits.
- `ToolCallCommitted` must be sent before its `ToolResultCommitted`.
- `ToolStarted`/`ToolEnded` 用于 UI progress；recovery depends on persist events.

---

## 5. Tool Runtime Routing

Tool execution receives complete `ToolCall` values only. It does not consume `GatewayEvent`.

For each tool call:

1. Send `DisplayEvent::ToolStarted`.
2. Execute the provider.
3. Send `DisplayEvent::ToolEnded`.
4. Build `Message::ToolResult`.
5. Reliably send `PersistEvent::ToolResultCommitted`.

Spawn tools additionally create child task execution through `AgentSpawner`; child task events flow through the same dispatch contract as root task events.

---

## 6. Hostd Event Bus

hostd consumes:

- `persist_rx`
- `display_rx`
- `lifecycle_rx`
- approval/user-interaction side events
- queue/model/auth/command events

hostd then:

- writes persist events to storage/state,
- updates task instance/spec projection/turn state from lifecycle events,
- forwards display/lifecycle/approval/queue/model/auth as `ServerMessage`,
- constructs session snapshots and task/agent view snapshots.

`ServerMessage` is the only wire event family exposed to TUI.

---

## 7. Multi-Agent Fan-In

All agent loops in one session share one `SessionChannels` fan-in:

```text
root agent_loop  ─┐
child agent_loop ─┼─> SessionChannels ─> hostd event bus
child agent_loop ─┘
```

This is valid because every event carries both runtime instance identity and spec identity:

- `session_id` where persistence requires it,
- `task_id` as the runtime task instance id,
- `agent_id` as the static agent spec/template id,
- `parent_task_id` / `source_agent_id` on `TaskEvent::Created`.

hostd demultiplexes this fan-in into its per-task live view store. TUI subscriptions read from hostd views; orchd does not expose per-agent wire streams directly to TUI.

---

## 8. Backpressure and Failure

Channel send policy is part of the architecture:

| Channel | Delivery policy | Failure handling |
|---|---|---|
| persist | reliable | fail turn or surface storage/runtime error |
| lifecycle | reliable for terminal events | fail or mark state inconsistent explicitly |
| display | reliable to hostd live view store | coalesce/materialize high-frequency deltas; never affects transcript |
| host side events | reliable while request is pending | cancel/fail pending workflow explicitly |

Silent loss is forbidden for persist and terminal lifecycle events.
