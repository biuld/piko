# Message Type Architecture

> 部分历史设计：本文档中的 `DisplayEvent` 与 `ServerMessage::Display` 已被移除。当前客户端消息模型见 `docs/session-output-projection.md` 与 `docs/hostd-tui-interaction.md`。

piko 的 message 类型体系定义从 LLM 原始输出到 transcript、到 session 持久化的全部类型。核心理念：**GatewayEvent 定义 LLM 内容原子，Message 类型与之对称，SessionTreeEntry 是 Message 的超集。**

Agent 架构见 `docs/agent-architecture.md`；identity 字段定义见 `docs/agent-identity.md`。本文件中的 `agent_id`、`task_id`、`parent_task_id`、`source_agent_id` 只描述类型承载位置，不重新定义字段语义。

---

## 1. 设计原则

### 1.1 GatewayEvent 作为规范词汇表

GatewayEvent 是 harness 和 LLM 之间的核心出入口，定义 LLM 能产出的内容原子类型：

```
GatewayEvent (LLM 内容原子)  ↔  Message / transcript (对称)
                                        ⊂
                              SessionTreeEntry (超集: transcript + session metadata)
```

- **对称的层**：GatewayEvent 的内容类型 ≡ Message 的内容类型。入和出用同一套词汇。
- **不对称的层**：SessionTreeEntry 除 Message 外还包含 ModelChange、Compaction 等 session 元数据。这些不在 transcript 里，不发回 LLM。

### 1.2 LLM 内容原子

一次 LLM generation 产出的内容类型：

| 类型 | OpenAI SSE | Anthropic SSE | 含义 |
|---|---|---|---|
| **Text** | `choices[].delta.content` | `content_block_delta(text)` | 可见文本 |
| **Thinking** | `choices[].delta.reasoning_content` | `content_block_delta(thinking)` | 推理链 |
| **ToolCall** | `choices[].delta.tool_calls[]` | `content_block_start(tool_use)` + deltas | 函数调用 |

Usage 和 stop_reason 为元数据，不是内容类型。

---

## 2. 类型层级

```
┌──────────────────────────────────────┐
│ Layer 0: LLM raw (HTTP SSE)          │  provider-specific
├──────────────────────────────────────┤
│ Layer 1: GatewayEvent (llmd)         │  ContentDelta | ReasoningDelta | ToolCallChunk
│          ↑ 对称 ↑                     │
├──────────────────────────────────────┤
│ Layer 2: Message (transcript)        │  User | Assistant | ToolCall | ToolResult
│          ⊂                           │
├──────────────────────────────────────┤
│ Layer 3: AgentMessage (extended)     │  Message | CustomAgentMessage
│          ⊂                           │
├──────────────────────────────────────┤
│ Layer 4: SessionTreeEntry (storage)  │  Message | ToolCall | ModelChange | ...
└──────────────────────────────────────┘
```

### 2.1 Layer 1: GatewayEvent

llmd 不做聚合，pass-through LLM 原始输出：

```rust
pub enum GatewayEvent {
    /// 文本增量
    ContentDelta(String),

    /// 思考增量
    ReasoningDelta(String),

    /// LLM 原始 tool call chunk
    /// 聚合由 orchd step dispatch / tool-call consumer 负责
    ToolCallChunk {
        id: String,         // call_id
        name: String,       // fn_name（首 chunk 有，续 chunk 为空）
        args_delta: String, // 部分参数字符串
    },

    /// Token 用量
    Usage(Usage),

    /// 流结束，带 stop_reason
    Done(String),

    /// 错误
    Error(String),
}
```

内容原子类型：3 种（ContentDelta, ReasoningDelta, ToolCallChunk）+ 元数据 2 种（Usage, Done）+ 错误 1 种（Error）。

### 2.2 Layer 2: Message（transcript）

transcript 类型集合与 GatewayEvent 内容原子对称：

```rust
pub enum Message {
    /// 用户输入
    User {
        content: MessageContent,
        timestamp: Option<i64>,
    },

    /// 助手回复（Text + Thinking only）
    Assistant {
        content: Vec<AssistantContentBlock>,
        model: String,
        provider: String,
        api: String,
        usage: Option<Usage>,
        stop_reason: Option<String>,
        error_message: Option<String>,
        timestamp: Option<i64>,
    },

    /// 工具调用
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
        model: Option<String>,
        provider: Option<String>,
        timestamp: Option<i64>,
    },

    /// 工具执行结果
    ToolResult {
        tool_call_id: String,
        tool_name: Option<String>,
        content: Vec<ContentBlock>,
        details: Option<serde_json::Value>,
        is_error: Option<bool>,
        timestamp: Option<i64>,
    },
}
```

### 2.3 ContentBlock（合并后）

原先 `ContentBlock` 和 `AssistantContentBlock` 分离是为了在类型层面防止 `ToolCall` 出现在 Assistant 中。`ToolCall` 已提取为独立的 `ToolCall` struct，两个类型变体完全相同，已合并为单一 `ContentBlock`：

```rust
pub enum ContentBlock {
    Text { text: String },
    Thinking { thinking: String, thinking_signature: Option<String> },
    Image { data: String, mime_type: String },
}

/// 独立 struct — tool provider execute() 参数
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    pub partial_json: Option<String>,
}
```

### 2.4 Layer 3: AgentMessage（扩展 transcript）

AgentMessage 是 transcript 的完整单元，包含标准 Message 和应用层自定义消息：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentMessage {
    Standard(Message),
    Custom(CustomAgentMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "camelCase")]
pub enum CustomAgentMessage {
    #[serde(rename = "bashExecution")]
    BashExecution {
        command: String,
        output: String,
        exit_code: Option<i32>,
        cancelled: bool,
        truncated: bool,
        exclude_from_context: bool,
        timestamp: i64,
    },
    #[serde(rename = "custom")]
    Custom {
        custom_type: String,
        content: CustomMessageContent,
        display: bool,
        details: Option<serde_json::Value>,
        timestamp: i64,
    },
    #[serde(rename = "branchSummary")]
    BranchSummary {
        summary: String,
        from_id: String,
        timestamp: i64,
    },
    #[serde(rename = "compactionSummary")]
    CompactionSummary {
        summary: String,
        tokens_before: u64,
        timestamp: i64,
    },
}
```

#### convert_to_llm()

每次 LLM 调用前，`AgentMessage[]` 转换为 `Message[]`。自定义消息转为 `Message::User` 文本形式，或根据标记跳过：

```rust
pub fn convert_to_llm(messages: &[AgentMessage]) -> Vec<Message> {
    messages.iter().filter_map(|am| match am {
        AgentMessage::Standard(m) => Some(m.clone()),
        AgentMessage::Custom(custom) => match custom {
            CustomAgentMessage::BashExecution { command, output, exclude_from_context, timestamp, .. } => {
                if *exclude_from_context { return None; }
                Some(Message::User {
                    content: MessageContent::String(
                        format!("Command executed: {}\nOutput:\n{}", command, output)
                    ),
                    timestamp: Some(*timestamp),
                })
            }
            CustomAgentMessage::BranchSummary { summary, timestamp, .. } => {
                Some(Message::User {
                    content: MessageContent::String(
                        format!("[Previous conversation summary]:\n{}", summary)
                    ),
                    timestamp: Some(*timestamp),
                })
            }
            CustomAgentMessage::CompactionSummary { summary, tokens_before, timestamp } => {
                Some(Message::User {
                    content: MessageContent::String(
                        format!("[Context compaction (~{} tokens)]:\n{}", tokens_before, summary)
                    ),
                    timestamp: Some(*timestamp),
                })
            }
            CustomAgentMessage::Custom { content, display, timestamp, .. } => {
                if !display { return None; }
                Some(Message::User {
                    content: MessageContent::String(/* extract text from content */),
                    timestamp: Some(*timestamp),
                })
            }
        },
    }).collect()
}
```

LLM 始终只看到 `user | assistant | toolCall | toolResult` 四元组。自定义消息存在于 transcript 和 session storage 中，对 LLM 透明。

### 2.5 Layer 4: SessionTreeEntry（持久化）

SessionTreeEntry 是 Message 的超集，包含 transcript 消息和 session metadata：

```rust
pub enum SessionTreeEntry {
    Message(MessageEntry),      // User | Assistant | ToolCall | ToolResult
    ToolCall(ToolCallEntry),

    // session metadata（不在 transcript 里）
    ThinkingLevelChange(ThinkingLevelChangeEntry),
    ModelChange(ModelChangeEntry),
    ActiveToolsChange(ActiveToolsChangeEntry),
    Compaction(CompactionEntry),
    BranchSummary(BranchSummaryEntry),
    Custom(CustomEntry),
    CustomMessage(CustomMessageEntry),
    Label(LabelEntry),
    SessionInfo(SessionInfoEntry),
    Leaf(LeafEntry),
}

pub struct MessageEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub message: Message,
}

pub struct ToolCallEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub parent_message_id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
}
```

多 agent transcript entry 必须同时携带 `agent_id` 与 `task_id`。字段语义、root `main`、spawn 缺省 `general`、以及 `name` 非 identity 规则见 `docs/agent-identity.md`。

### 2.6 Transcript 结构

```
User("read this file")
  → Assistant { content: [Text("Let me read it...")] }
    → ToolCall { id: "call_1", name: "read", args: {path: "/foo"} }
      → ToolResult { tool_call_id: "call_1", content: "file content" }
        → Assistant { content: [Text("The file contains...")] }
          → ToolCall { id: "call_2", name: "write", args: {...} }
            → ToolResult { tool_call_id: "call_2", content: "ok" }
              → ...
```

匹配 OpenAI API 的 message 格式，可直接序列化为 genai ChatMessage。

---

## 3. 事件类型（agent runtime 产出）

agent runtime 产出三类 typed events，通过类型窄化的 channel 分别投递给 hostd。hostd 再把需要展示给 TUI 的事件包装为 `ServerMessage`。

多 agent 场景下还有一个独立的 orchd 运行时实体：`Supervisor`。它不产出 transcript message 类型本身，但负责：

- 分配 spawned task 的 `task_id`
- 维护 `task_id -> handle` 全局注册表
- 启动 task driver，把 task stream 接到 session senders
- 保存 task result cache，供 `spawn` / `poll_task` 读取
- 路由 `steer_task`、关闭、取消等跨 task 控制请求

因此：

- `spawn` 是父 task 发起的控制请求，不是父 task 直接创建 child task。
- `TaskEvent::Created` 的 runtime instance identity 由 supervisor 落地注册。
- `task_id` 是 runtime 节点 identity；supervisor registry 是运行时索引，不是 DAG 节点。

### 3.1 Runtime Input / Control Types

除了 hostd-facing typed events，多 agent runtime 还依赖稳定的输入和控制面。
代码不为这些动作定义一个统一的 `RuntimeEffect` 或 `SupervisorRequest` union；它们在第一接收层直接分流。

```rust
/// supervisor 注册后交给 task orchestrator 的启动输入
pub struct AgentTask {
    pub id: Option<TaskId>,
    pub target_agent_id: AgentId,
    pub prompt: String,
    pub source: TaskSource,
    pub parent_task_id: Option<TaskId>,
    pub history: Option<Vec<Message>>,
    pub host_context: Option<HostTaskContext>,
}

/// 对现有 task 注入新的输入
pub struct TaskSteerMessage {
    pub source_task_id: TaskId,
    pub source_agent_id: AgentId,
    pub message: String,
    pub senders: Option<DispatchSenders>,
}

/// 单 task control channel 的窄类型输入
pub enum TaskControlMessage {
    Steer(TaskSteerMessage),
    Close,
    Reopen,
}

/// 跨 task 请求由 supervisor 实现的 port 直接分流
pub trait AgentSpawner {
    async fn spawn(...);
    async fn spawn_detached(...);
    async fn poll_task(...);
    async fn steer_task(...);
}
```

约束：

- cancellation 使用独立的 `CancellationToken`，不塞入 `TaskControlMessage`。
- 这些类型描述的是 orchd runtime 的输入与控制面，不是 transcript message。
- 它们不直接进入 session storage，除非被投影为 lifecycle 或 committed message。
- `spawn` / `steer_task` 的第一接收者是 supervisor registry，不是目标 task 自己。

### 3.2 Hostd-facing Typed Events

```rust
/// persist channel — hostd 消费并转换为 SessionTreeEntry
pub enum PersistEvent {
    Finalized { session_id, message_id, task_id, agent_id, message: Message },
    ToolCallCommitted { session_id, message_id, task_id, agent_id, parent_message_id, message: Message },
    ToolResultCommitted { session_id, message_id, task_id, agent_id, message: Message },
    TaskEventCommitted(TaskEvent),
}

/// display channel — orchd → TUI 渲染事件，不含持久化语义
pub enum DisplayEvent {
    // message streaming
    TextDelta { task_id, agent_id, message_id, content_index: u32, delta: String },
    MessageStart { message_id, task_id, agent_id, role: MessageRole },
    MessageEnd { message_id, task_id, agent_id, stop_reason: Option<String>, error_message: Option<String> },
    ThinkingDelta { task_id, agent_id, message_id, content_index: u32, delta: String },
    Finalized { message_id, task_id, agent_id, content: Vec<ContentBlock>, usage: Option<Usage>, stop_reason: Option<String>, error_message: Option<String> },
    ToolCallDelta { task_id, agent_id, message_id, content_index: u32, tool_call_id: String, delta: String },

    // tool lifecycle（已展平）
    ToolStarted { task_id, agent_id, tool_call_id, tool_name, args, parent_message_id? },
    ToolEnded { task_id, agent_id, tool_call_id, tool_name, result, is_error },

    // interaction（已展平）
    InteractionRequested { task_id, agent_id, interaction_id, tool_call_id, title?, questions, require_confirm, auto_resolution_ms? },
    InteractionResolved { task_id, agent_id, interaction_id, status },
}

/// lifecycle channel — 编排事件
pub enum LifecycleEvent {
    Task(TaskEvent),
    Turn(TurnEvent),
}

/// ServerMessage — hostd → TUI 统一线协议
pub enum ServerMessage {
    CommandResponse { command_id, result },
    Auth(AuthEvent),
    Display(DisplayEvent),
    Persist(PersistEvent),
    TaskLifecycle(TaskEvent),
    TurnLifecycle(TurnEvent),
    Approval(ApprovalEvent),
    Queue(QueueEvent),
    Model(ModelEvent),
    AgentConnected { agent_id, task_id, parent_task_id?, name, role },
    AgentDisconnected { agent_id, task_id, reason },
}
```

`TaskEvent` 是完整的 task 状态与关系事件集：

```rust
pub enum TaskEvent {
    Created {
        session_id,
        task_id,
        agent_id,
        parent_task_id,
        source_agent_id,
        prompt,
        turn_id,
        timestamp,
    },
    Started { session_id, task_id, agent_id, timestamp },
    Idle { session_id, task_id, agent_id, total_steps, summary, timestamp },
    Completed { session_id, task_id, agent_id, total_steps, summary, final_status, timestamp },
    Failed { session_id, task_id, agent_id, error, timestamp },
    Cancelled { session_id, task_id, agent_id, timestamp },
    Closed { session_id, task_id, agent_id, timestamp },
    Reopened { session_id, task_id, agent_id, timestamp },
    Joined { session_id, task_id, parent_task_id, result, timestamp },
    Steered { session_id, task_id, source_task_id, source_agent_id, message, timestamp },
}
```

其中 `agent_id` 是 spec/template id，`task_id` 是该次运行的 instance id。所有 display、persist、approval、interaction 事件都必须同时携带 `task_id` 和 `agent_id`，以便 hostd 同时完成运行时路由和 template display/context 解析。

### 3.3 Host-managed Side Events

以下事件不由 orchd 的 agent runtime 直接生产，但属于 hostd 管理并推给 TUI 的稳定事件面：

```rust
pub enum ServerMessage {
    CommandResponse { command_id, result },
    Auth(AuthEvent),
    Display(DisplayEvent),
    Persist(PersistEvent),
    TaskLifecycle(TaskEvent),
    TurnLifecycle(TurnEvent),
    Approval(ApprovalEvent),
    Queue(QueueEvent),
    Model(ModelEvent),
    AgentConnected { agent_id, task_id, parent_task_id?, name, role },
    AgentDisconnected { agent_id, task_id, reason },
}
```

其余 host-managed event family 的完整 variant 为：

| Event family | Variants |
|---|---|
| `TurnEvent` | `Started \| Completed \| Failed \| Cancelled` |
| `ApprovalEvent` | `Requested \| Resolved` |
| `QueueEvent` | `Updated` |
| `ModelEvent` | `ConfigChanged` |
| `AuthEvent` | `LoginDeviceCode \| LoginSuccess \| LoginFailed \| LoggedOut` |
| `CommandResult` | `Empty \| SessionCreated \| SessionOpened \| SessionListed \| SessionNavigated \| StateSnapshot \| ModelListed \| CommandCatalogListed \| AgentSpecListed \| AgentListed \| AgentSubscribed \| ConfigEntry` |

其中：

- `ApprovalEvent`：hostd 管理的工具审批状态。
- `QueueEvent`：turn / task 排队和调度状态。
- `ModelEvent`：模型、provider 或 catalog 变更。
- `AuthEvent`：provider 登录、token、鉴权状态。
- `CommandResponse`：TUI 发起命令后的同步/异步结果。

这些 side events 可能与 display/lifecycle 同时影响 TUI，但它们不是 transcript 事实来源。

### 3.4 完整事件清单与 consumer 对应

| 事件 / 控制面 | transcript consumer | persist consumer | display consumer | lifecycle consumer | supervisor registry | ServerMessage variant | 说明 |
|---|---|---|---|---|---|---|---|
| `AgentTask` | initial user message merge | no | no | created/started | yes | none | 创建并启动一个 task runtime |
| `TaskSteerMessage` | user/steering message merge | no | no | steered | yes | none | 唤醒或继续一个 idle/failed task |
| `CancellationToken` | no | no | no | cancelled | yes | none | 取消 task 当前 runtime |
| `TaskControlMessage::Close` / `Reopen` | no | no | no | closed/reopened | yes | none | 控制 task 是否可继续接受输入 |
| `Finalized` | assistant message merge | yes | yes | no | no | Display | Assistant 完成；persist 用于恢复，display 用于最终渲染 |
| `ToolCallCommitted` | tool call merge | yes | no | no | no | none | 工具调用持久化；TUI 进度看 `ToolStarted` |
| `ToolResultCommitted` | tool result merge | yes | no | no | no | none | 工具结果持久化；TUI 进度看 `ToolEnded` |
| `TaskLifecycle` | no | metadata | no | yes | observe / index task state | TaskLifecycle | Task 生命周期 / task DAG metadata |
| `TextDelta` | no | no | yes | no | no | Display | 文本增量 |
| `ThinkingDelta` | no | no | yes | no | no | Display | 思考增量 |
| `MessageStart` / `MessageEnd` | no | no | yes | no | no | Display | Stream 开始/结束 |
| `ToolCallDelta` | no | no | yes | no | no | Display | 工具参数增量 |
| `ToolStarted` / `ToolEnded` | no | no | yes | no | no | Display | 工具生命周期 |
| `InteractionRequested` / `InteractionResolved` | no | no | yes | no | no | Display | 用户交互 live rendering |
| `TurnEvent` | no | no | no | yes | no | TurnLifecycle | Turn 生命周期 |
| `spawn` / `spawn_detached` / `poll_task` / `steer_task` | no | no | no | no | yes | none | orchd 内部跨 task 控制面；不是 hostd-facing typed event |
| `ApprovalEvent` | no | no | no | no | no | Approval | hostd 管理的审批状态 |
| `QueueEvent` | no | no | no | no | no | Queue | hostd 管理的排队/调度状态 |
| `ModelEvent` | no | no | no | no | no | Model | hostd 管理的模型状态 |
| `AuthEvent` | no | no | no | no | no | Auth | hostd 管理的鉴权状态 |
| `CommandResponse` | no | no | no | no | no | CommandResponse | 命令执行响应 |

### 事件可靠性

- `PersistEvent::Finalized`、`ToolCallCommitted`、`ToolResultCommitted` 是 resume/transcript 事实来源，必须可靠投递到 hostd。
- `DisplayEvent` 是 live rendering，不参与 transcript 恢复。
- `TaskEvent` 是 task DAG 事实来源。运行时 agent tree 从 `task_id`、`parent_task_id`、`agent_id`、`source_agent_id` 派生；节点 identity 是 `task_id`。
- `senders=None` 只表示本地 collecting sink，不表示可以静默丢弃 typed events。
- supervisor registry 负责跨 task 控制与注册，不替代 persist/display/lifecycle 三条 hostd-facing typed channels。
- hostd 是 `ApprovalEvent`、`QueueEvent`、`ModelEvent`、`AuthEvent` 的状态 owner。

---

## 4. Resume 路径

resume 时从 hostd snapshot 重建 transcript 和 UI state。transcript 只取 message 类 entry，跳过不应发回 LLM 的 metadata：

```
SessionTreeEntry → Message（与 GatewayEvent 对称）→ genai ChatMessage

  Message(Assistant) → Message::Assistant（Text/Thinking）
  Message(ToolCall)  → Message::ToolCall
  Message(ToolResult) → Message::ToolResult
  ModelChange        → 跳过
  Compaction         → 跳过
```

运行时 agent tree 从 task lifecycle metadata 重建；pending approvals 和 interactions 从 hostd state 重建；display deltas 不参与 resume。

多 agent storage 使用 session directory：

```text
~/.piko/sessions/<encoded-cwd>/<session-id>/
  main.jsonl
  <agent-id>.jsonl
  tasks.json
```

- `main.jsonl` 保存 session header、user/root transcript、branch/leaf/session metadata。
- `<agent-id>.jsonl` 是按 agent spec/template 分片的 committed transcript。entry 内的 `task_id` 决定它属于哪个运行时实例；文件分片不是 runtime registry，也不替代 supervisor 的全局 task 注册表。
- `tasks.json` 是 task DAG sidecar，key 为 `task_id`，value 保存 `agent_id`、`parent_task_id`、`source_agent_id`、prompt、status、terminal result/error、updated timestamp。

resume 时 hostd 先合并 JSONL transcript，再读取 `tasks.json` 重建 task DAG 和 task views。TUI 不从文件名推断运行时关系，也不把 `agent_id` 当作树节点主键。

---

## 5. 类型总览

| Type | 定义 |
|---|---|
| `ContentBlock` | `Text \| Thinking \| Image` |
| `ToolCall` | `{ id, name, arguments, partial_json }` — execute() 参数 |
| `Message` | `User \| Assistant \| ToolCall \| ToolResult` |
| `AgentMessage` | `Standard(Message) \| Custom(CustomAgentMessage)` |
| `GatewayEvent` | `ContentDelta \| ReasoningDelta \| ToolCallChunk \| Usage \| Done \| Error` |
| `AgentTask` | supervisor 注册后交给 task runtime 的启动输入 |
| `TaskSteerMessage` | 对已有 task 的输入注入，包含 source identity 和 senders |
| `TaskControlMessage` | `Steer \| Close \| Reopen`；只用于单 task control channel |
| `CancellationToken` | model step、工具执行和 task runtime 的取消输入 |
| `AgentSpawner` | supervisor 实现的 `spawn \| spawn_detached \| poll_task \| steer_task` port |
| `PersistEvent` | `Finalized \| ToolCallCommitted \| ToolResultCommitted \| TaskEventCommitted(TaskEvent)` |
| `DisplayEvent` | `TextDelta \| MessageStart \| MessageEnd \| ThinkingDelta \| Finalized \| ToolCallDelta \| ToolStarted \| ToolEnded \| InteractionRequested \| InteractionResolved` |
| `LifecycleEvent` | `Task(TaskEvent) \| Turn(TurnEvent)` |
| `ServerMessage` | `CommandResponse \| Auth \| Display \| Persist \| TaskLifecycle \| TurnLifecycle \| Approval \| Queue \| Model \| AgentConnected \| AgentDisconnected` |
| `SessionTreeEntry` | `Message \| ToolCall \| ThinkingLevelChange \| ModelChange \| ...` (11 种) |
