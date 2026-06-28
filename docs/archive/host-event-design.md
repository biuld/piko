# HostEvent 设计文档

> Historical design note. This document captures an earlier event-model design
> and should not be used as current implementation status. For current hostd
> planning, protocol ownership, and known runtime risks, see
> `docs/architecture/hostd-global-plan.md`.

## 1. 核心概念

### 1.1 Session（会话）

持久化的对话容器。跨多次用户交互存活，可 fork / branch / navigate。

```
Session "abc123"
  cwd: "/Users/biu/projects/demo"
  transcript: [msg_1, msg_2, msg_3, ...]
```

存储为 JSONL：`~/.piko/sessions/<encoded-cwd>/<session-id>.jsonl`

### 1.2 Turn（轮次）

**一次用户交互的完整生命周期。** 用户提交 prompt 开始，所有 agent 任务完成后结束。

```
Turn "turn_1"
  ├─ user prompt: "review and fix the code"
  │
  ├─ Task "main" (agent="main", root)         ← 根任务
  │   ├─ Step 1: model → text "Let me review..."
  │   ├─ Step 2: model → tool_call(spawn, "code-reviewer")
  │   ├─ Step 3: await sub-task result
  │   ├─ Step 4: model → tool_call(write)
  │   └─ TaskCompleted
  │
  └─ Task "sub-1" (agent="code-reviewer")     ← 子任务
       ├─ Step 1: model → tool_call(read)
       ├─ Step 2: model → text "Issues found:..."
       ├─ TaskCompleted
       └─ TaskTranscriptCommitted → 合入 main transcript
```

**Turn 的关键语义**：
- 一个 Turn 包含一个根 Task（`parent_task_id = None`）
- 根 Task 可以通过 tool call 派生子 Task（`spawn` / `spawn_detached`）
- Task 之间形成树状结构
- Turn 的结束 = 根 Task + 所有子孙 Task 全部完成
- 单 agent 场景：Turn 只有根 Task，无子 Task

### 1.3 Task（任务）

**一个 agent 的执行单元。** 拥有独立的 agent_id、transcript、状态。

```
Task "main"
  agent_id: "main"
  status: running → completed
  transcript: [msg(user), msg(assistant), msg(tool_result), ...]
  parent_task_id: None               ← 根任务
  children: ["sub-1"]

Task "sub-1"
  agent_id: "code-reviewer"
  status: running → completed
  transcript: [...]
  parent_task_id: "main"             ← 子任务
```

- Task 是 agent loop 的执行载体
- Task 的 `source` 可以是 `User`（根任务）或 `Agent { agent_id, task_id }`（子任务）
- Task 完成时，其 transcript 通过 `TaskTranscriptCommitted` 合入父 Task 的上下文

### 1.4 Step（模型步）

Agent loop 中的一次模型调用。非事件概念，仅用于描述内部执行。

### 1.5 Message（消息）

Transcript 中的一条记录。持久化在 session JSONL 中。

| 角色 | 来源 | 示例 |
|---|---|---|
| `user` | 用户输入 / steering | `"review and fix the code"` |
| `assistant` | 模型输出 | text + tool_calls |
| `tool_result` | 工具执行结果 | `{ "content": "file contents..." }` |

### 1.6 概念关系图

```
Session "abc123"
├─ Turn "turn_1"     ← 用户提交 "写一个函数"
│   └─ Task "main" (root)
│       ├─ Step 1 → Message(assistant) + Message(tool_result)
│       ├─ Step 2 → Message(assistant)  ← 结束
│       └─ TaskCompleted
│
├─ Turn "turn_2"     ← 用户提交 "review and fix"
│   ├─ Task "main" (root)
│   │   ├─ Step 1 → tool_call(spawn, "reviewer")
│   │   ├─ Task "sub-1" (child)
│   │   │   ├─ Step 1 → Message(assistant) "found 3 issues"
│   │   │   ├─ TaskCompleted
│   │   │   └─ TaskTranscriptCommitted
│   │   ├─ Step 2 → tool_call(write)  ← 使用 sub-1 的结果
│   │   └─ TaskCompleted
│   └─ TurnCompleted
│
├─ Turn "turn_3"     ← followUp 自动触发
│   └─ Task "main" (root)
│       └─ ...
```

---

## 2. 事件设计

### 2.1 总览：21 种事件

```
┌──────────────────────────────────────────────────────────────────┐
│ Domain events（持久化到 JSONL，用于 state rebuild）—— 15 种        │
├──────────────────────────────────────────────────────────────────┤
│ UserMessageSubmitted    用户消息                                   │
│ AssistantMessageCompleted  模型完成 assistant 消息                 │
│ ToolResultCommitted      工具执行结果                              │
│ TurnStarted / TurnCompleted / TurnFailed / TurnCancelled          │
│ TaskCreated / TaskStarted / TaskCompleted / TaskFailed /          │
│   TaskCancelled / TaskTranscriptCommitted / TaskJoined /          │
│   TaskSteered                                                     │
│ SessionCreated           会话创建                                 │
│ QueueUpdate              队列变化                                 │
│ ModelConfigChanged       模型配置变更                              │
├──────────────────────────────────────────────────────────────────┤
│ Streaming events（不持久化，实时推 TUI）—— 8 种                     │
├──────────────────────────────────────────────────────────────────┤
│ MessageStart / MessageEnd  消息流式边界                            │
│ TextDelta / ThinkingDelta  逐 token 增量                          │
│ ToolStart / ToolEnd        工具执行状态                            │
│ ApprovalRequested / ApprovalResolved  审批                         │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 事件字段公约

所有 streaming 事件携带 `task_id` + `agent_id`，用于区分多 agent 输出。

所有 domain 事件携带 `session_id`（持久化路由）和 `timestamp`（排序）。

---

## 3. Domain Events（12 种）

### 3.1 消息事件

```rust
UserMessageSubmitted {
    session_id: String,
    message_id: String,      // UUID
    task_id: String,          // 所属 task
    text: String,
    timestamp: i64,
}

AssistantMessageCompleted {
    session_id: String,
    message_id: String,      // UUID
    task_id: String,
    agent_id: String,         // 哪个 agent 产生的
    text: String,             // 完整文本（所有 TextDelta 的拼接）
    tool_calls: Vec<ToolCallRef>,
    model: String,
    provider: String,
    usage: Option<Usage>,
    timestamp: i64,
}

ToolResultCommitted {
    session_id: String,
    message_id: String,      // tool_result 消息的 UUID
    task_id: String,
    agent_id: String,
    tool_call_id: String,    // 关联的 tool call
    tool_name: String,
    content: Value,
    is_error: bool,
    timestamp: i64,
}

struct ToolCallRef {
    id: String,
    name: String,
    args: Value,
}
```

### 3.2 Turn 生命周期

```rust
TurnStarted {
    session_id: String,
    turn_id: String,         // UUID，本轮次的唯一标识
    root_task_id: String,    // 根 task 的 ID
    timestamp: i64,
}

TurnCompleted {
    session_id: String,
    turn_id: String,
    total_tasks: u32,        // 本轮次共创建了多少个 task（含子任务）
    timestamp: i64,
}

TurnFailed {
    session_id: String,
    turn_id: String,
    error: String,
    timestamp: i64,
}

TurnCancelled {
    session_id: String,
    turn_id: String,
    timestamp: i64,
}
```

### 3.3 Task 生命周期

Task 有三种创建方式，对应不同的事件序列：

| 操作 | 创建者 | 父 task 行为 | 事件序列 |
|---|---|---|---|
| **根 task**（用户 prompt） | hostd | — | `TaskCreated` → `TaskStarted` → ... → `TaskCompleted` |
| **`spawn`**（阻塞） | tool call | 阻塞等待子 task | `TaskCreated` → `TaskStarted` → ... → `TaskCompleted` → `TaskTranscriptCommitted` |
| **`spawn_detached`**（异步） | tool call | 立即继续 | `TaskCreated` → ...（异步）→ `TaskStarted` → ... → `TaskCompleted` → ...（join）→ `TaskJoined` |

```rust
/// Task 已创建，进入队列，尚未开始执行。
/// 对 spawn_detached 尤为重要：创建和执行之间有间隙。
TaskCreated {
    session_id: String,
    task_id: String,          // UUID
    agent_id: String,         // 目标 agent
    parent_task_id: Option<String>,  // None = 根任务
    source_agent_id: Option<String>, // 谁 spawn 的（None = 用户）
    prompt: String,           // task 的 prompt 文本
    turn_id: String,
    timestamp: i64,
}

/// Agent 开始执行此 task。由 agent runner 单次 emit。
TaskStarted {
    session_id: String,
    task_id: String,
    agent_id: String,
    timestamp: i64,
}

/// Agent 正常完成。由 agent runner 单次 emit。
TaskCompleted {
    session_id: String,
    task_id: String,
    agent_id: String,
    total_steps: u32,
    summary: String,
    final_status: String,     // "completed" | "aborted" | "error"
    timestamp: i64,
}

TaskFailed {
    session_id: String,
    task_id: String,
    agent_id: String,
    error: String,
    timestamp: i64,
}

TaskCancelled {
    session_id: String,
    task_id: String,
    agent_id: String,
    timestamp: i64,
}

/// 子 task 的 transcript 合入父 task 上下文。
/// 由 agent runner emit，仅在子 task（parent_task_id != None）时产生。
TaskTranscriptCommitted {
    session_id: String,
    task_id: String,          // 子 task 的 ID
    agent_id: String,
    parent_task_id: String,   // 合入哪个父 task
    messages: Vec<Message>,    // 子 agent 的完整 transcript
    summary: String,
    final_status: String,
    timestamp: i64,
}

/// 父 task 通过 await_task 收集了 detached task 的结果。
/// 由 orchestration 层 emit，标记 join 完成。
TaskJoined {
    session_id: String,
    task_id: String,          // 被 join 的 task ID
    parent_task_id: String,   // 发起 join 的父 task
    result: Value,            // join 返回的结果
    timestamp: i64,
}

/// 父 task（或用户）向运行中的 task 注入了一条 steering 消息。
/// steering 进入目标 task 的队列，在其下一个 step 被消费。
TaskSteered {
    session_id: String,
    task_id: String,           // 被 steer 的 task
    source_task_id: String,    // 谁 steer 的
    source_agent_id: String,
    message: String,
    timestamp: i64,
}
```

### 3.4 Session & Config

```rust
SessionCreated {
    session_id: String,
    cwd: String,
    timestamp: i64,
}

QueueUpdate {
    session_id: String,
    steer_count: u32,
    follow_up_count: u32,
    next_turn_count: u32,
    steer_preview: Option<String>,
    follow_up_preview: Option<String>,
}

ModelConfigChanged {
    session_id: String,
    model_id: String,
    provider: String,
    timestamp: i64,
}
```

---

## 4. Streaming Events（6 种）

所有 streaming 事件携带 `task_id` + `agent_id`，不持久化。

```rust
MessageStart {
    task_id: String,
    agent_id: String,
    message_id: String,
    role: MessageRole,       // "assistant" | "tool_result"
}

MessageEnd {
    task_id: String,
    agent_id: String,
    message_id: String,
    stop_reason: Option<String>,
}

TextDelta {
    task_id: String,
    agent_id: String,
    message_id: String,
    delta: String,
}

ThinkingDelta {
    task_id: String,
    agent_id: String,
    message_id: String,
    delta: String,
}

ToolStart {
    task_id: String,
    agent_id: String,
    tool_call_id: String,
    tool_name: String,
    args: Value,
    parent_message_id: Option<String>,
}

ToolEnd {
    task_id: String,
    agent_id: String,
    tool_call_id: String,
    tool_name: String,
    result: Value,
    is_error: bool,
}

ApprovalRequested {
    task_id: String,
    agent_id: String,
    approval_id: String,
    tool_name: String,
    tool_args: Value,
}

ApprovalResolved {
    task_id: String,
    agent_id: String,
    approval_id: String,
    decision: ApprovalDecision,
}
```

---

## 5. 完整生命周期示例

### 5.1 单 agent Turn

```
用户输入 "写一个 Rust 函数"

  [Domain] UserMessageSubmitted { message_id:"m1", task_id:"t1", text:"写一个 Rust 函数" }
  [Domain] TurnStarted { turn_id:"turn_1", root_task_id:"t1" }
  [Domain] TaskStarted { task_id:"t1", agent_id:"main" }

      ├─ Step 1
      │   [Streaming] MessageStart { task_id:"t1", agent_id:"main", message_id:"m2", role:"assistant" }
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", message_id:"m2", delta:"Let" }
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", message_id:"m2", delta:" me" }
      │   [Streaming] ToolStart { task_id:"t1", agent_id:"main", tool_call_id:"tc1", name:"read" }
      │   [Streaming] MessageEnd { message_id:"m2", stop_reason:"tool_calls" }
      │   [Domain] AssistantMessageCompleted { message_id:"m2", text:"Let me", tool_calls:[tc1] }
      │
      │   [Streaming] ToolEnd { tool_call_id:"tc1", result:"...", is_error:false }
      │   [Domain] ToolResultCommitted { message_id:"m3", tool_call_id:"tc1" }
      │
      ├─ Step 2
      │   [Streaming] MessageStart { task_id:"t1", agent_id:"main", message_id:"m4", ... }
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", message_id:"m4", delta:"Done!" }
      │   [Streaming] MessageEnd { message_id:"m4", stop_reason:"end_turn" }
      │   [Domain] AssistantMessageCompleted { message_id:"m4", text:"Done!", tool_calls:[] }

  [Domain] TaskCompleted { task_id:"t1", agent_id:"main", total_steps:2, summary:"Wrote a Rust function", final_status:"completed" }
  [Domain] TurnCompleted { turn_id:"turn_1", total_tasks:1 }
```

### 5.2 多 agent Turn — spawn（阻塞）

```
用户输入 "review and fix the code"

  [Domain] UserMessageSubmitted { message_id:"m1", task_id:"t1", text:"review and fix" }
  [Domain] TurnStarted { turn_id:"turn_2", root_task_id:"t1" }
  [Domain] TaskCreated { task_id:"t1", agent_id:"main", parent_task_id:null, source_agent_id:null, prompt:"review and fix" }
  [Domain] TaskStarted { task_id:"t1", agent_id:"main" }

      ├─ main Step 1
      │   [Streaming] MessageStart { task_id:"t1", agent_id:"main", ... }
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", delta:"Let me spawn..." }
      │   [Streaming] ToolStart { task_id:"t1", agent_id:"main", name:"spawn" }
      │   [Streaming] MessageEnd { message_id:"m2", stop_reason:"tool_calls" }
      │   [Domain] AssistantMessageCompleted { message_id:"m2", text:"Let me spawn...", tool_calls:[{name:"spawn"}] }
      │
      │   ┌─ spawn (blocking) ──────────────────────────────┐
      │   [Domain] TaskCreated { task_id:"t2", agent_id:"reviewer", parent_task_id:"t1", source_agent_id:"main", prompt:"review the code" }
      │   [Domain] TaskStarted { task_id:"t2", agent_id:"reviewer" }
      │   │
      │   │   reviewer Step 1
      │   │   [Streaming] MessageStart { task_id:"t2", agent_id:"reviewer", ... }
      │   │   [Streaming] TextDelta { task_id:"t2", agent_id:"reviewer", delta:"Found 3 issues:" }
      │   │   [Streaming] MessageEnd { message_id:"m3", stop_reason:"end_turn" }
      │   │   [Domain] AssistantMessageCompleted { message_id:"m3", text:"Found 3 issues:" }
      │   │
      │   [Domain] TaskCompleted { task_id:"t2", agent_id:"reviewer", total_steps:1, summary:"Found 3 issues", final_status:"completed" }
      │   [Domain] TaskTranscriptCommitted { task_id:"t2", agent_id:"reviewer", parent_task_id:"t1", messages:[...], summary:"Found 3 issues..." }
      │   └────────────────────────────────────────────────────┘
      │
      │   [Streaming] ToolEnd { task_id:"t1", tool_call_id:"spawn_1", result:{task_id:"t2",...} }
      │   [Domain] ToolResultCommitted { message_id:"m4", tool_call_id:"spawn_1" }
      │
      ├─ main Step 2
      │   [Streaming] MessageStart { task_id:"t1", agent_id:"main", message_id:"m5", ... }
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", delta:"Based on review..." }
      │   [Streaming] ToolStart { task_id:"t1", agent_id:"main", name:"write" }
      │   [Streaming] MessageEnd { message_id:"m5", stop_reason:"tool_calls" }
      │   [Domain] AssistantMessageCompleted { message_id:"m5", ... }
      │   [Streaming] ToolEnd { task_id:"t1", tool_call_id:"write_1", ... }
      │   [Domain] ToolResultCommitted { ... }
      │
      ├─ main Step 3
      │   [Streaming] MessageStart { task_id:"t1", agent_id:"main", ... }
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", delta:"All fixed!" }
      │   [Streaming] MessageEnd { message_id:"m6", stop_reason:"end_turn" }
      │   [Domain] AssistantMessageCompleted { message_id:"m6", text:"All fixed!" }

  [Domain] TaskCompleted { task_id:"t1", agent_id:"main", total_steps:3, summary:"All issues fixed", final_status:"completed" }
  [Domain] TurnCompleted { turn_id:"turn_2", total_tasks:2 }
```

### 5.3 spawn_detached + await_task（异步）


用户输入 "review and fix the code"（main agent 使用 spawn_detached）

  [Domain] UserMessageSubmitted { message_id:"m1", task_id:"t1", text:"review and fix" }
  [Domain] TurnStarted { turn_id:"turn_2", root_task_id:"t1" }
  [Domain] TaskCreated { task_id:"t1", agent_id:"main", parent_task_id:null, source_agent_id:null, prompt:"review and fix" }
  [Domain] TaskStarted { task_id:"t1", agent_id:"main" }

      ├─ main Step 1
      │   [Streaming] MessageStart { task_id:"t1", agent_id:"main", ... }
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", delta:"Let me spawn..." }
      │   [Streaming] ToolStart { task_id:"t1", agent_id:"main", name:"spawn_detached" }
      │
      │   [Domain] TaskCreated { task_id:"t2", agent_id:"reviewer", parent_task_id:"t1", source_agent_id:"main", prompt:"review the code" }
      │   ↑ 子 task 已创建，但 main 不等待，立即继续
      │
      │   [Streaming] ToolEnd { task_id:"t1", tool_call_id:"dt1", result:{task_id:"t2",status:"detached"} }
      │   [Domain] ToolResultCommitted { message_id:"m3", tool_call_id:"dt1", content:{task_id:"t2",status:"detached"} }
      │   [Streaming] MessageEnd { message_id:"m2", stop_reason:"end_turn" }
      │   [Domain] AssistantMessageCompleted { message_id:"m2", text:"I spawned a reviewer..." }
      │
      │   ════════════════ main 继续，reviewer 异步运行 ════════════════
      │
      │   [Domain] TaskStarted { task_id:"t2", agent_id:"reviewer" }   ← 异步
      │   [Streaming] MessageStart { task_id:"t2", agent_id:"reviewer", ... }
      │   [Streaming] TextDelta { task_id:"t2", agent_id:"reviewer", delta:"Found issues..." }
      │   [Streaming] MessageEnd { task_id:"t2", agent_id:"reviewer", ... }
      │
      ├─ main Step 2（与 reviewer 并行）
      │   [Streaming] MessageStart { task_id:"t1", agent_id:"main", ... }
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", delta:"While waiting..." }
      │   [Streaming] ToolStart { task_id:"t1", agent_id:"main", name:"await_task", args:{task_ids:["t2"]} }
      │   ↑ main 显式 join reviewer
      │
      │   [Domain] TaskCompleted { task_id:"t2", agent_id:"reviewer", summary:"Found issues", final_status:"completed" }
      │   [Domain] TaskTranscriptCommitted { task_id:"t2", parent_task_id:"t1", messages:[...] }
      │   [Domain] TaskJoined { task_id:"t2", parent_task_id:"t1", result:{...} }
      │   ↑ join 完成
      │
      │   [Streaming] ToolEnd { task_id:"t1", tool_call_id:"at1", result:{task_id:"t2",...} }
      │   [Domain] ToolResultCommitted { message_id:"m4", tool_call_id:"at1" }
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", delta:"Based on review..." }
      │   [Streaming] MessageEnd { message_id:"m5", stop_reason:"end_turn" }
      │   [Domain] AssistantMessageCompleted { message_id:"m5", ... }

  [Domain] TaskCompleted { task_id:"t1", agent_id:"main", total_steps:2, summary:"All fixed", final_status:"completed" }
  [Domain] TurnCompleted { turn_id:"turn_2", total_tasks:2 }


### 5.4 审批流程

```
  [Streaming] ToolStart { task_id:"t1", agent_id:"main", tool_call_id:"tc2", name:"bash", args:"rm -rf /" }
  [Streaming] ApprovalRequested { task_id:"t1", agent_id:"main", approval_id:"apr1", tool_name:"bash", ... }
      │
      │  ← 用户确认 (TUI → hostd: approval.respond)
      │
  [Streaming] ApprovalResolved { task_id:"t1", agent_id:"main", approval_id:"apr1", decision:"accept" }
  [Streaming] ToolEnd { task_id:"t1", agent_id:"main", tool_call_id:"tc2", result:"...", ... }
  [Domain] ToolResultCommitted { task_id:"t1", agent_id:"main", ... }
```

---

### 5.5 spawn_detached + steer（监督模式）

```
用户输入 "review and fix the code"
main agent spawn_detached reviewer → 审查 reviewer 输出 → steer 纠正方向

  [Domain] UserMessageSubmitted { message_id:"m1", task_id:"t1", text:"review and fix" }
  [Domain] TurnStarted { turn_id:"turn_3", root_task_id:"t1" }
  [Domain] TaskCreated { task_id:"t1", agent_id:"main", ... }
  [Domain] TaskStarted { task_id:"t1", agent_id:"main" }

      ├─ main Step 1: spawn_detached
      │   [Streaming] ToolStart { task_id:"t1", agent_id:"main", name:"spawn_detached", args:{agent_id:"reviewer"} }
      │   [Domain] TaskCreated { task_id:"t2", agent_id:"reviewer", parent_task_id:"t1", source_agent_id:"main", prompt:"review the code" }
      │   [Streaming] ToolEnd { tool_call_id:"dt1", result:{task_id:"t2", status:"detached"} }
      │   [Domain] ToolResultCommitted { message_id:"m3", tool_call_id:"dt1" }

      │   ═══════════ reviewer 异步启动 ═══════════
      │   [Domain] TaskStarted { task_id:"t2", agent_id:"reviewer" }
      │   [Streaming] TextDelta { task_id:"t2", agent_id:"reviewer", delta:"I'll use React..." }

      ├─ main Step 2: 观察 reviewer 输出 → steer
      │   [Streaming] TextDelta { task_id:"t1", agent_id:"main", delta:"Hmm, wrong direction..." }
      │   [Streaming] ToolStart { task_id:"t1", agent_id:"main", name:"steer_task", args:{task_id:"t2", message:"Use SolidJS, not React"} }
      │
      │   [Domain] TaskSteered { task_id:"t2", source_task_id:"t1", source_agent_id:"main", message:"Use SolidJS, not React" }
      │   ↑ steering 写入 reviewer 的队列
      │
      │   [Streaming] ToolEnd { tool_call_id:"st1", result:{status:"steered"} }
      │   [Domain] ToolResultCommitted { message_id:"m4", tool_call_id:"st1" }

      │   ═══════════ reviewer 下个 step 消费 steering ═══════════
      │   [Streaming] TextDelta { task_id:"t2", agent_id:"reviewer", delta:"Actually, let's use SolidJS..." }
      │   ↑ 方向已纠正

      ├─ main Step 3: await_task
      │   [Streaming] ToolStart { task_id:"t1", agent_id:"main", name:"await_task", args:{task_ids:["t2"]} }
      │   [Domain] TaskCompleted { task_id:"t2", agent_id:"reviewer", summary:"Used SolidJS", final_status:"completed" }
      │   [Domain] TaskTranscriptCommitted { task_id:"t2", parent_task_id:"t1", messages:[...] }
      │   [Domain] TaskJoined { task_id:"t2", parent_task_id:"t1", result:{...} }
      │   [Streaming] ToolEnd { tool_call_id:"at1", result:{...} }

  [Domain] TaskCompleted { task_id:"t1", agent_id:"main", total_steps:3, summary:"All fixed with SolidJS", final_status:"completed" }
  [Domain] TurnCompleted { turn_id:"turn_3", total_tasks:2 }


## 6. 监督模型（Supervision Model）

### 6.1 概念

每个 Task 都拥有独立的 **steering queue**。Pusher 可以是：
- **User** → 用户通过 TUI `/steer <task_id> <message>` 命令
- **Parent agent** → 父 task 通过 `steer_task` tool

这实现了**对称的监督模型**：main agent 监督 sub agent 的方式，和 user 监督 main agent 完全一致。

```
User ──steer──→ main agent's queue ──→ main agent loop
                     │
                     ├─ spawn_detached → reviewer's queue ──→ reviewer agent loop
                     │       │
                     └─ steer_task ────┘
```

### 6.2 新增 Tool

在 `TaskControlProvider` 中增加：

| Tool | 参数 | 效果 | 事件 |
|---|---|---|---|
| `steer_task` | `task_id`, `message` | push steering 到目标 task 的队列 | `TaskSteered` |
| `cancel_task` | `task_id` | 取消目标 task | `TaskCancelled` |
| `inspect_task` | `task_id` | 返回目标 task 状态快照（status, steps, transcript preview） | 无（同步返回值） |

### 6.3 Agent Loop 集成

每个 agent loop 在 step 之间检查自己的 steering queue：

```
loop {
    // 1. 检查 steering queue
    if let Some(steering) = queue.pop() {
        transcript.push(Message::user(steering.message));
        emit TaskSteeringConsumed { ... };  // 可选
    }
    
    // 2. 模型 step
    let step = model.execute(transcript);
    
    // 3. 工具执行
    for tool_call in step.tool_calls {
        match tool_call.name {
            "steer_task" => push_to_task_queue(tool_call.args.task_id, tool_call.args.message),
            "cancel_task" => cancel_task(tool_call.args.task_id),
            // ...
        }
    }
}
```

### 6.4 与当前代码的对照

当前代码已经有 `HostQueueController` 处理 user → main agent 的 steering。新设计将同一机制泛化到所有 task：

| 当前 | 新设计 |
|---|---|
| `HostQueueController.steer()` — user → main | `steer_task` tool — parent → sub |
| Queue 只存在于 host-runtime 的 HostState | Queue 是每个 agent loop 的内建能力 |
| `QueueUpdate` 事件通知 TUI | 同，但可携带 `task_id` 以支持多 task |


## 7. Rust enum 定义

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostEvent {
    // ═══ Domain: Messages ═══
    UserMessageSubmitted {
        session_id: String,
        message_id: String,
        task_id: String,
        text: String,
        timestamp: i64,
    },
    AssistantMessageCompleted {
        session_id: String,
        message_id: String,
        task_id: String,
        agent_id: String,
        text: String,
        tool_calls: Vec<ToolCallRef>,
        model: String,
        provider: String,
        usage: Option<Usage>,
        timestamp: i64,
    },
    ToolResultCommitted {
        session_id: String,
        message_id: String,
        task_id: String,
        agent_id: String,
        tool_call_id: String,
        tool_name: String,
        content: serde_json::Value,
        is_error: bool,
        timestamp: i64,
    },

    // ═══ Domain: Turn ═══
    TurnStarted {
        session_id: String,
        turn_id: String,
        root_task_id: String,
        timestamp: i64,
    },
    TurnCompleted {
        session_id: String,
        turn_id: String,
        total_tasks: u32,
        timestamp: i64,
    },
    TurnFailed {
        session_id: String,
        turn_id: String,
        error: String,
        timestamp: i64,
    },
    TurnCancelled {
        session_id: String,
        turn_id: String,
        timestamp: i64,
    },

    // ═══ Domain: Task ═══
    TaskCreated {
        session_id: String,
        task_id: String,
        agent_id: String,
        parent_task_id: Option<String>,
        source_agent_id: Option<String>,
        prompt: String,
        turn_id: String,
        timestamp: i64,
    },
    TaskStarted {
        session_id: String,
        task_id: String,
        agent_id: String,
        timestamp: i64,
    },
    TaskCompleted {
        session_id: String,
        task_id: String,
        agent_id: String,
        total_steps: u32,
        summary: String,
        final_status: String,
        timestamp: i64,
    },
    TaskFailed {
        session_id: String,
        task_id: String,
        agent_id: String,
        error: String,
        timestamp: i64,
    },
    TaskCancelled {
        session_id: String,
        task_id: String,
        agent_id: String,
        timestamp: i64,
    },
    TaskTranscriptCommitted {
        session_id: String,
        task_id: String,
        agent_id: String,
        parent_task_id: String,
        messages: Vec<Message>,
        summary: String,
        final_status: String,
        timestamp: i64,
    },
    TaskJoined {
        session_id: String,
        task_id: String,
        parent_task_id: String,
        result: serde_json::Value,
        timestamp: i64,
    },
    TaskSteered {
        session_id: String,
        task_id: String,
        source_task_id: String,
        source_agent_id: String,
        message: String,
        timestamp: i64,
    },

    // ═══ Domain: Session & Config ═══
    SessionCreated {
        session_id: String,
        cwd: String,
        timestamp: i64,
    },
    QueueUpdate {
        session_id: String,
        steer_count: u32,
        follow_up_count: u32,
        next_turn_count: u32,
        steer_preview: Option<String>,
        follow_up_preview: Option<String>,
    },
    ModelConfigChanged {
        session_id: String,
        model_id: String,
        provider: String,
        timestamp: i64,
    },

    // ═══ Streaming: Message ═══
    MessageStart {
        task_id: String,
        agent_id: String,
        message_id: String,
        role: MessageRole,
    },
    MessageEnd {
        task_id: String,
        agent_id: String,
        message_id: String,
        stop_reason: Option<String>,
    },
    TextDelta {
        task_id: String,
        agent_id: String,
        message_id: String,
        delta: String,
    },
    ThinkingDelta {
        task_id: String,
        agent_id: String,
        message_id: String,
        delta: String,
    },

    // ═══ Streaming: Tool ═══
    ToolStart {
        task_id: String,
        agent_id: String,
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        parent_message_id: Option<String>,
    },
    ToolEnd {
        task_id: String,
        agent_id: String,
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    },

    // ═══ Streaming: Approval ═══
    ApprovalRequested {
        task_id: String,
        agent_id: String,
        approval_id: String,
        tool_name: String,
        tool_args: serde_json::Value,
    },
    ApprovalResolved {
        task_id: String,
        agent_id: String,
        approval_id: String,
        decision: ApprovalDecision,
    },
}

// ── Companion types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallRef {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    Assistant,
    ToolResult,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Accept,
    Decline,
    AcceptSession,
    AcceptWorkspace,
}
```

---

## 8. Domain vs Streaming 分类

```rust
impl HostEvent {
    pub fn is_domain(&self) -> bool {
        matches!(
            self,
            HostEvent::UserMessageSubmitted { .. }
                | HostEvent::AssistantMessageCompleted { .. }
                | HostEvent::ToolResultCommitted { .. }
                | HostEvent::TurnStarted { .. }
                | HostEvent::TurnCompleted { .. }
                | HostEvent::TurnFailed { .. }
                | HostEvent::TurnCancelled { .. }
                | HostEvent::TaskCreated { .. }
                | HostEvent::TaskStarted { .. }
                | HostEvent::TaskCompleted { .. }
                | HostEvent::TaskFailed { .. }
                | HostEvent::TaskCancelled { .. }
                | HostEvent::TaskTranscriptCommitted { .. }
                | HostEvent::TaskJoined { .. }
                | HostEvent::TaskSteered { .. }
                | HostEvent::SessionCreated { .. }
                | HostEvent::QueueUpdate { .. }
                | HostEvent::ModelConfigChanged { .. }
        )
    }

    pub fn is_streaming(&self) -> bool {
        !self.is_domain()
    }
}
```

**Domain events**: 写入 JSONL journal，用于 state rebuild。
**Streaming events**: 仅实时推送，不入库。

---

## 9. EventBus & Session Journal

### 9.1 EventBus

```rust
use tokio::sync::broadcast;

pub struct EventBus {
    tx: broadcast::Sender<Arc<HostEvent>>,
    journal: Option<EventJournal>,
}

impl EventBus {
    pub fn publish(&self, event: HostEvent) {
        // Domain events → persist to JSONL
        if event.is_domain() {
            if let Some(ref journal) = self.journal {
                let ev = event.clone();
                let journal = journal.clone();
                tokio::spawn(async move { let _ = journal.append(&ev).await; });
            }
        }
        // All events → broadcast to subscribers
        let _ = self.tx.send(Arc::new(event));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<HostEvent>> {
        self.tx.subscribe()
    }
}
```

### 9.2 Session JSONL 格式

每行一个 domain event：

```jsonl
{"type":"session_created","session_id":"abc123","cwd":"/Users/biu/projects/demo","timestamp":1719000000000}
{"type":"user_message_submitted","session_id":"abc123","message_id":"m1","task_id":"t1","text":"review and fix","timestamp":1719000001000}
{"type":"turn_started","session_id":"abc123","turn_id":"turn_2","root_task_id":"t1","timestamp":1719000001001}
{"type":"task_started","session_id":"abc123","task_id":"t1","agent_id":"main","parent_task_id":null,"turn_id":"turn_2","timestamp":1719000001002}
{"type":"assistant_message_completed","session_id":"abc123","message_id":"m2","task_id":"t1","agent_id":"main","text":"Let me spawn...","tool_calls":[{"id":"tc1","name":"spawn","args":{"agent_id":"reviewer","prompt":"review"}}],"model":"claude-sonnet-4","provider":"anthropic","timestamp":1719000002000}
{"type":"task_started","session_id":"abc123","task_id":"t2","agent_id":"reviewer","parent_task_id":"t1","turn_id":"turn_2","timestamp":1719000002001}
{"type":"assistant_message_completed","session_id":"abc123","message_id":"m3","task_id":"t2","agent_id":"reviewer","text":"Found 3 issues","tool_calls":[],"model":"claude-sonnet-4","provider":"anthropic","timestamp":1719000003000}
{"type":"task_completed","session_id":"abc123","task_id":"t2","agent_id":"reviewer","total_steps":1,"timestamp":1719000003001}
{"type":"task_transcript_committed","session_id":"abc123","task_id":"t2","agent_id":"reviewer","parent_task_id":"t1","messages":[...],"summary":"Found 3 issues","final_status":"completed","timestamp":1719000003002}
{"type":"tool_result_committed","session_id":"abc123","message_id":"m4","task_id":"t1","agent_id":"main","tool_call_id":"tc1","tool_name":"spawn","content":{"task_id":"t2"},"is_error":false,"timestamp":1719000003003}
{"type":"assistant_message_completed","session_id":"abc123","message_id":"m5","task_id":"t1","agent_id":"main","text":"All fixed!","tool_calls":[],"model":"claude-sonnet-4","provider":"anthropic","timestamp":1719000005000}
{"type":"task_completed","session_id":"abc123","task_id":"t1","agent_id":"main","total_steps":2,"timestamp":1719000005001}
{"type":"turn_completed","session_id":"abc123","turn_id":"turn_2","total_tasks":2,"timestamp":1719000005002}
```

### 9.3 State Rebuild

从 journal 重放 domain events 重建 session state：

```rust
fn rebuild_session(events: &[HostEvent]) -> SessionState {
    let mut state = SessionState::default();
    for event in events {
        match event {
            HostEvent::SessionCreated { session_id, cwd, .. } => {
                state.id = session_id.clone();
                state.cwd = cwd.clone();
            }
            HostEvent::TurnStarted { turn_id, .. } => {
                state.active_turn_id = Some(turn_id.clone());
            }
            HostEvent::TurnCompleted { turn_id, .. }
            | HostEvent::TurnFailed { turn_id, .. }
            | HostEvent::TurnCancelled { turn_id, .. } => {
                if state.active_turn_id.as_deref() == Some(turn_id) {
                    state.active_turn_id = None;
                }
            }
            HostEvent::TaskStarted { task_id, agent_id, parent_task_id, .. } => {
                state.active_tasks.insert(task_id.clone(), TaskState {
                    agent_id: agent_id.clone(),
                    parent_task_id: parent_task_id.clone(),
                    status: TaskStatus::Running,
                });
            }
            HostEvent::TaskCompleted { task_id, .. } => {
                if let Some(t) = state.active_tasks.get_mut(task_id) {
                    t.status = TaskStatus::Completed;
                }
            }
            HostEvent::UserMessageSubmitted { message_id, text, .. } => {
                state.messages.push(TranscriptMessage::User { id: message_id.clone(), text: text.clone() });
            }
            HostEvent::AssistantMessageCompleted { message_id, text, tool_calls, .. } => {
                state.messages.push(TranscriptMessage::Assistant { id: message_id.clone(), text: text.clone(), tool_calls: tool_calls.clone() });
            }
            HostEvent::ToolResultCommitted { message_id, tool_call_id, tool_name, content, .. } => {
                state.messages.push(TranscriptMessage::ToolResult { id: message_id.clone(), tool_call_id: tool_call_id.clone(), tool_name: tool_name.clone(), content: content.clone() });
            }
            _ => {}
        }
    }
    state
}
```

---

## 10. TUI 消费

TS 侧 `hostEventToTuiEvent()` 映射：

```typescript
function hostEventToTuiEvent(event: HostEvent): TuiEvent | TuiEvent[] | null {
    switch (event.type) {
        // Turn lifecycle
        case "turn_started":
            return { type: "stream_started" };
        case "turn_completed":
        case "turn_cancelled":
            return { type: "stream_settled" };
        case "turn_failed":
            return [{ type: "stream_settled" }, { type: "turn_failed", error: event.error }];

        // Task lifecycle → agent panel updates
        case "task_started":
            return { type: "task_started", taskId: event.task_id, agentId: event.agent_id, parentTaskId: event.parent_task_id };
        case "task_completed":
            return { type: "task_completed", taskId: event.task_id, agentId: event.agent_id };
        case "task_transcript_committed":
            return { type: "task_transcript_committed", taskId: event.task_id, parentTaskId: event.parent_task_id, messages: event.messages };

        // Streaming → direct (单 agent 时 agent_id 总是 "main")
        case "text_delta":
            return { type: "assistant_delta", delta: event.delta, agentId: event.agent_id };
        case "thinking_delta":
            return { type: "thinking_delta", delta: event.delta, agentId: event.agent_id };
        case "tool_start":
            return { type: "tool_call_started", id: event.tool_call_id, name: event.tool_name, args: event.args, agentId: event.agent_id };
        case "tool_end":
            return { type: "tool_call_ended", id: event.tool_call_id, name: event.tool_name, result: event.result, isError: event.is_error, agentId: event.agent_id };

        // Approval
        case "approval_requested":
            return { type: "approval_needed", toolEntityId: event.approval_id, callId: event.approval_id, toolName: event.tool_name, toolArgs: event.tool_args, agentId: event.agent_id };
        case "approval_resolved":
            return { type: "approval_resolved", toolEntityId: event.approval_id, callId: event.approval_id, decision: event.decision };

        // Queue
        case "queue_update":
            return { type: "queue_update", steerCount: event.steer_count, followUpCount: event.follow_up_count, ... };

        // Session
        case "session_created":
            return { type: "session_info_updated", sessionId: event.session_id };

        // Domain messages → transcript reducer 消费
        case "user_message_submitted":
        case "assistant_message_completed":
        case "tool_result_committed":
            return null; // 由 transcript reducer 处理

        // Message boundaries → single-agent 时可不消费
        case "message_start":
        case "message_end":
            return null;
    }
}
```

---

## 11. 删除清单

| 删除 | 原因 |
|---|---|
| `OrchestratorEvent` (14 种) | 合并到 HostEvent + Task 事件 |
| `HostEvent` pi-compat (16 种, camelCase) | 旧 pi 协议 |
| `OrchEvent` (12 种) | 冗余中间层 |
| `RuntimeEvent` (8 种) | 冗余中间层 |
| `OrchSourcingEvent` (14 种) | 合并到 HostEvent domain subset |
| `host_protocol::HostEvent` (15 种, kebab-case) | 替换为统一的 HostEvent |
| `AgentActor` + `ActorSystem` | hostd 直接管理 agent loop |
| `subscribe()` / `subscribe_orch()` | EventBus::subscribe() 替代 |
| `OrchRuntime` trait | hostd 直接调用 orchd 组件 |
| `CoreOrchestratorRuntime` | 同上 |
| TS `HostRuntimeEvent` + `projectHostEvent()` | 不再需要投影层 |
| "Run" 概念 | 被 Turn + Task 吸收 |

**5 层 64 种变体 → 1 层 21 种变体**
