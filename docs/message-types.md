# Message Type Architecture

piko 的 message 类型体系定义从 LLM 原始输出到 transcript、到 session 持久化的全部类型。核心理念：**GatewayEvent 定义 LLM 内容原子，Message 类型与之对称，SessionTreeEntry 是 Message 的超集。**

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
    /// 聚合由下游 tool executor 负责
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

原先 `ContentBlock` 和 `AssistantContentBlock` 分离是为了在类型层面防止 `ToolCall` 出现在 Assistant 中。`ToolCall` 已提取为独立的 `ToolCallData` struct，两个类型变体完全相同，已合并为单一 `ContentBlock`：

```rust
pub enum ContentBlock {
    Text { text: String },
    Thinking { thinking: String, thinking_signature: Option<String> },
    Image { data: String, mime_type: String },
}

/// 独立 struct — tool provider execute() 参数
pub struct ToolCallData {
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

pub struct ToolCallEntry {
    pub id: EntryId,
    pub parent_id: Option<EntryId>,
    pub timestamp: String,
    pub agent_id: Option<String>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub parent_message_id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
}
```

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

## 3. 事件类型（dispatcher 产出）

dispatcher 产出两类事件，通过类型窄化的 channel 分别投递：

```rust
/// persist channel — hostd 消费
pub enum PersistEvent {
    Finalized { session_id, message_id, task_id, agent_id, message: Message },
    ToolCallCommitted { session_id, message_id, task_id, agent_id, parent_message_id, message: Message },
    ToolResultCommitted { session_id, message_id, task_id, agent_id, message: Message },
    TaskLifecycle(TaskEvent),
}

/// display channel — TUI 消费
pub enum DisplayEvent {
    TextDelta { task_id, agent_id, message_id, content_index: u32, delta: String },
    MessageStart { message_id, task_id, agent_id, role: MessageRole },
    MessageEnd { message_id, task_id, agent_id, stop_reason: Option<String> },
    ThinkingDelta { task_id, agent_id, message_id, content_index: u32, delta: String },
    Finalized { message_id, task_id, agent_id, content: Vec<AssistantContentBlock>, usage: Option<Usage>, stop_reason: Option<String> },
    ToolCallCommitted { message_id, task_id, agent_id, parent_message_id, message: Message },
    ToolCallDelta { task_id, agent_id, message_id, content_index: u32, tool_call_id: String, delta: String },
    ToolEvent(ToolEvent),
    InteractionEvent(InteractionEvent),
    TaskLifecycle(TaskEvent),
    TurnLifecycle(TurnEvent),
}
```

### 事件类型与 consumer 对应

| 事件 | persist | display | 说明 |
|---|---|---|---|
| `Finalized` | ✅ | ✅ (Arc) | Assistant 完成（TUI markdown re-parse） |
| `ToolCallCommitted` | ✅ | ✅ | 工具调用提交（含完整 Message） |
| `ToolResultCommitted` | ✅ | — | 工具结果落盘 |
| `TaskLifecycle` | ✅ | ✅ (Arc) | Task 生命周期 |
| `TextDelta` | — | ✅ | 文本增量（TUI typewriter） |
| `ThinkingDelta` | — | ✅ | 思考增量 |
| `MessageStart` / `MessageEnd` | — | ✅ | Stream 开始/结束 |
| `ToolCallDelta` | — | ✅ | 工具参数增量 |
| `ToolEvent` | — | ✅ | 工具生命周期 |
| `InteractionEvent` | — | ✅ | 用户交互 |
| `TurnLifecycle` | — | ✅ | Turn 生命周期 |

---

## 4. Resume 路径

resume 时从 SessionTreeEntry 重建 transcript。只取 Message 和 ToolCall entry，跳过 metadata：

```
SessionTreeEntry → Message（与 GatewayEvent 对称）→ genai ChatMessage

  Message(Assistant) → Message::Assistant（Text/Thinking）
  ToolCall           → Message::ToolCall
  Message(ToolResult) → Message::ToolResult
  ModelChange        → 跳过
  Compaction         → 跳过
```

---

## 5. 类型总览

| Type | 定义 |
|---|---|
| `ContentBlock` | `Text \| Thinking \| Image` |
| `ToolCallData` | `{ id, name, arguments, partial_json }` — execute() 参数，已从 ContentBlock 分离 |
| `Message` | `User \| Assistant \| ToolCall \| ToolResult` |
| `AgentMessage` | `Standard(Message) \| Custom(CustomAgentMessage)` |
| `GatewayEvent` | `ContentDelta \| ReasoningDelta \| ToolCallChunk \| Usage \| Done \| Error` |
| `PersistEvent` | `Finalized \| ToolCallCommitted \| ToolResultCommitted \| TaskLifecycle(TaskEvent)` |
| `DisplayEvent` | `TextDelta \| MessageStart \| MessageEnd \| ThinkingDelta \| Finalized \| ToolCallCommitted \| ToolCallDelta \| ToolEvent(ToolEvent) \| InteractionEvent(InteractionEvent) \| TaskLifecycle(TaskEvent) \| TurnLifecycle(TurnEvent)` |
| `SessionTreeEntry` | `Message \| ToolCall \| ThinkingLevelChange \| ModelChange \| ...` (11 种) |
