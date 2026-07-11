## 7. Event Model

系统有三个事件平面和一个 command response 平面。

### 7.1 Durable Facts

用于 JSONL、恢复和审计：

```text
MessageCommitted
TaskLifecycleCommitted
WorkLifecycleCommitted
```

### 7.2 Session Observation Output

session observation 的最终消费者目前主要是 hostd/TUI，但不能把所有输出都建模成纯渲染细节。对外统一暴露一个 `SessionOutputStream`，语义上区分可靠的结构化通知与临时实时增量：

```rust
pub enum SessionOutput {
    Event(SessionEventEnvelope),
    Delta(RealtimeDeltaEnvelope),
}
```

可靠事件：

```rust
pub enum SessionEvent {
    TaskChanged {
        snapshot: TaskSnapshot,
    },
    WorkChanged {
        snapshot: WorkSnapshot,
    },
    MessageCommitted {
        message_id: MessageId,
        work_id: WorkId,
        role: MessageRole,
    },
    ToolCommitted {
        message_id: MessageId,
        work_id: WorkId,
        tool_call_id: ToolCallId,
    },
    InteractionRequested {
        request: InteractionRequest,
    },
    InteractionResolved {
        resolution: InteractionResolution,
    },
}
```

实时增量：

```rust
pub enum RealtimeDelta {
    MessageStarted {
        role: MessageRole,
    },
    Text {
        content_index: u32,
        delta: String,
    },
    Thinking {
        content_index: u32,
        delta: String,
    },
    ToolCall {
        content_index: u32,
        tool_call_id: ToolCallId,
        delta: String,
    },
    MessageEnded {
        stop_reason: Option<String>,
        error_message: Option<String>,
    },
}
```

二者都会经 hostd 转发给 TUI 或其他 client，但 delivery semantics 不同：

| Lane | Semantics |
|---|---|
| reliable event | 低频、结构化、可靠通知；断线后可由 snapshot/task log 修正 |
| realtime delta | 高频、临时、允许 subscriber lag 时丢失；不用于恢复 |

public API 可以合并为一个 `SessionOutputStream`；内部必须保留两个不同 QoS lane，避免大量 token delta 阻塞可靠事件。

这些输出都只是 observation：

- hostd durable state 在 `PersistSink` commit 时更新，不依赖 `SessionEvent` 构建。
- supervisor runtime state 通过 task runtime/internal observer 更新，不依赖 session output 回流。
- `SessionEvent` 是状态变化发生后的 notification，不是状态机输入。
- `RealtimeDelta` 只驱动实时渲染，不承担恢复语义。

### 7.3 Runtime Lifecycle

用于 live status 和 supervision：

```text
TaskCreated
TaskStarted
TaskIdle
TaskClosed
TaskReopened
TaskTerminated
WorkStarted
WorkSucceeded
WorkFailed
WorkCancelled
```

task/work lifecycle 首先作为 durable fact 经 `PersistSink` commit，并通过内部 lifecycle observer 更新 supervisor。commit 完成后可以投影为 `SessionEvent::TaskChanged` 或 `SessionEvent::WorkChanged`。不再建立 public lifecycle stream，也不让 hostd 从 lifecycle notification 反向构建权威状态。

### 7.4 Command Acknowledgement

用于告诉调用者命令是否被接受：

```text
TaskCreated receipt
InputReceipt
ControlApplied
ControlRejected
```

command acknowledgement 不进入 transcript。

### 7.5 Message Persist Event

最终目标是统一 committed message：

```rust
pub enum PersistEvent {
    MessageCommitted {
        session_id: SessionId,
        task_id: TaskId,
        agent_id: AgentId,
        work_id: WorkId,
        task_seq: u64,
        message_id: MessageId,
        parent_message_id: Option<MessageId>,
        message: Message,
        committed_at: i64,
    },
    TaskEventCommitted(TaskEvent),
    WorkEventCommitted(WorkEvent),
}
```

迁移第一阶段可以保留现有 variants，并新增：

```rust
UserCommitted {
    session_id,
    message_id,
    task_id,
    agent_id,
    message,
}
```

然后通过统一 helper 将 `UserCommitted`、`Finalized`、`ToolCallCommitted` 和 `ToolResultCommitted` 归一化成 committed message view。

### 7.6 Lifecycle Does Not Own Transcript

`TaskEvent::Created.prompt` 和 `TaskEvent::Steered.message` 可以暂时作为兼容或审计字段保留，但不得作为恢复来源。

禁止：

```text
TaskEvent::Created → resume prompt
TaskEvent::Steered → resume user message
```

正确关系：

```text
submit_input
  ├─ MessageCommitted(Message::User)
  └─ lifecycle/audit event referencing message_id
```

---

