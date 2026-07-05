# Hostd ↔ TUI Interaction

hostd 和 TUI 通过 JSON-lines stdio 双向通信。本文档描述线协议、命令/事件体系、会话生命周期，以及多 agent 的 pub/sub API 设计。

## 0. hostd 内部通道调度

hostd 消费 orchd 返回的 `SessionChannels`（三个 mpsc channel），自己做 dispatch：

```
orchd ──→ SessionChannels
              │
              ├── display channel   ──→ hostd ──→ ServerMessage::Display   ──→ merge ──→ TUI
              ├── lifecycle channel ──→ hostd ──→ ServerMessage::Lifecycle ──→ merge ──→ TUI
              └── persist channel   ──→ hostd ──→ persist_from_event       ──→ JSONL
```

- **display / lifecycle**：包装为 `ServerMessage` variant，合并后通过 JSONL stdout 推给 TUI
- **persist**：直接消费，转换为 `SessionTreeEntry` 写入 JSONL，不转发给 TUI

多 agent 时，每个 agent 有自己的 `SessionChannels`。hostd 维护 `HashMap<AgentId, SessionChannels>`，TUI 通过 `AgentSubscribe` 命令选择要接收哪个 agent 的 stream。

---

## 1. 线协议

```
TUI ── JSONL stdin  ──→ hostd   (commands)
TUI ←── JSONL stdout ──  hostd   (events)
```

每行一个 JSON 对象。TUI 发 Command，hostd 回 ServerMessage。

### 1.1 Command（TUI → hostd）

```rust
enum Command {
    SessionCreate { command_id, cwd },
    SessionOpen { command_id, session_id, session_path? },
    SessionList { command_id },
    SessionNavigate { command_id, session_id, target_entry_id },
    SessionRename { command_id, session_id, name },
    SessionCompact { command_id, session_id },
    StateSnapshot { command_id, session_id },
    TurnSubmit { command_id, session_id, text },
    TurnCancel { command_id, session_id, turn_id },
    TurnSteer { command_id, session_id, task_id, message },
    ModelList { command_id },
    ModelSet { command_id, provider, model_id, thinking_level? },
    AuthLogin { command_id, provider },
    AuthLogout { command_id, provider },
    ApprovalRespond { command_id, session_id, approval_id, decision },
    UserInteractionRespond { command_id, session_id, interaction_id, response },
    ConfigGet { command_id, namespace },
    ConfigUpdate { command_id, namespace, value },
    CommandCatalog { command_id },
}
```

每条 Command 带 `command_id`，hostd 回 `CommandResponse { command_id, result }`。

### 1.2 ServerMessage（hostd → TUI）

```rust
enum ServerMessage {
    CommandResponse { command_id, result: Result<CommandResult, String> },
    Auth(AuthEvent),
    Display(DisplayEvent),         // orchd → TUI 渲染事件
    Lifecycle(LifecycleEvent),     // 编排事件（Task / Turn 生命周期）
    Approval(ApprovalEvent),       // 工具审批
    Queue(QueueEvent),             // turn 队列状态
    Model(ModelEvent),             // 模型配置变更
}
```

`Display`、`Lifecycle`、`Approval` 在 turn 执行期间持续推送。`CommandResponse`、`Auth`、`Queue`、`Model` 为独立管理事件。

### 1.3 事件分类

| 类别 | variant | 来源 | 消费者 |
|---|---|---|---|
| **渲染流** | `Display` | orchd display channel | TUI timeline |
| **编排** | `Lifecycle` | orchd lifecycle channel + hostd | TUI agent 状态 |
| **审批** | `Approval` | orchd approval gateway | TUI approval panel |
| **队列** | `Queue` | hostd session state | TUI bottom bar |
| **模型** | `Model` | hostd config | TUI model selector |
| **认证** | `Auth` | hostd auth flow | TUI status |
| **回执** | `CommandResponse` | hostd command handler | TUI effect handler |

---

## 2. 单 Agent 会话流程

### 2.1 打开或创建 session

```
TUI:  SessionCreate { cwd }  ──→  hostd
TUI: ←──  CommandResponse { SessionCreated }  ── hostd
TUI: ←──  Display(TextDelta / ToolStarted / ...)  ── orchd → hostd
```

或打开已有 session：

```
TUI:  SessionOpen { session_id }  ──→  hostd
TUI: ←──  CommandResponse { SessionOpened { snapshot } }  ── hostd
      TUI 用 snapshot 恢复整个 timeline
```

### 2.2 Turn 执行

```
TUI 用户在 editor 输入 → 按 Enter

TUI 本地渲染用户消息（push_user），然后：

TUI:  TurnSubmit { text }  ──→  hostd
      hostd 持久化用户 entry → 调用 orchd.run()

TUI: ←──  Lifecycle(Turn(Started))    ── hostd
TUI: ←──  Lifecycle(Task(Created))    ── orchd → hostd
TUI: ←──  Display(TextDelta)          ── orchd → hostd
TUI: ←──  Display(ToolStarted)         ── orchd → hostd
TUI: ←──  Display(ToolEnded)           ── orchd → hostd
TUI: ←──  Display(InteractionRequested) ── orchd → hostd
           ...TUI 用户作答...
TUI:  UserInteractionRespond ──→ hostd → orchd
TUI: ←──  Display(InteractionResolved) ── orchd → hostd
TUI: ←──  Display(Finalized)          ── orchd → hostd
TUI: ←──  Lifecycle(Task(Completed))  ── orchd → hostd
TUI: ←──  Lifecycle(Turn(Completed))  ── hostd
```

### 2.3 审批流程

```
TUI: ←──  Approval(Requested { approval_id, tool_name, tool_args })
      TUI 弹出 approval panel
      用户选择 Accept / AcceptSession / AcceptWorkspace / Decline

TUI:  ApprovalRespond { approval_id, decision }  ──→ hostd → orchd
TUI: ←──  Approval(Resolved { approval_id, decision })
```

---

## 3. 多 Agent Pub/Sub API

### 3.1 设计目标

- TUI 支持同时观看多个 agent 的 timeline
- 切换 agent 不暂停、不 reload、不 reconcile
- agent 动态创建（spawn）时 TUI 自动感知

### 3.2 新增 Command

| Command | 参数 | 响应 |
|---|---|---|
| `AgentList` | — | `AgentListed { agents }` |
| `AgentSubscribe { agent_id }` | 要订阅的 agent | 返回该 agent 的 `Stream<ServerMessage>` |
| `AgentUnsubscribe { agent_id }` | 取消订阅 | `CommandResponse::Empty` |

### 3.3 新增 Event

| Event | 触发时机 |
|---|---|
| `AgentConnected { agent_id, parent_agent_id?, name, role }` | 新 agent 上线（TaskCreated） |
| `AgentDisconnected { agent_id, reason }` | agent 结束（TaskCompleted / Failed） |

### 3.4 线协议扩展

多 agent 场景下，`ServerMessage` 需要携带 `agent_id` 以便 TUI 路由到对应 timeline：

```rust
// 方案 A：wrapper
enum TuiEvent {
    AgentStream { agent_id, event: ServerMessage },
    AgentConnected { agent_id, /* ... */ },
    AgentDisconnected { agent_id, reason },
    CommandResponse { command_id, result },
    // ...其他全局事件
}

// 方案 B：扩展 ServerMessage
struct Envelope {
    agent_id: Option<AgentId>,  // None = 全局事件
    event: ServerMessage,
}
```

### 3.5 TUI 内部模型

```rust
struct TuiState {
    active_agent: AgentId,
    agents: HashMap<AgentId, AgentView>,
    agent_list: Vec<AgentInfo>,           // AgentListed 返回
}

struct AgentView {
    timeline: Timeline,
    display_stream: Pin<Box<dyn Stream<Item = ServerMessage>>>,
}
```

### 3.6 流程示例

```
TUI 启动，打开 session

TUI:  AgentList ──→ hostd
TUI: ←── AgentListed { agents: [{ id: "main", status: "idle" }] }

用户提交 turn：

TUI:  TurnSubmit { text } ──→ hostd
TUI: ←── AgentConnected { agent_id: "main", name: "main", role: "assistant" }
      hostd 自动订阅 main agent，开始推送事件

TUI: ←── Lifecycle(Turn(Started))
TUI: ←── Display(TextDelta)
      ...

tool executor spawn child：

TUI: ←── AgentConnected { agent_id: "child_a", parent_agent_id: "main", name: "child_a", role: "assistant" }
      TUI 自动在 agent list 中显示

用户切换到 child：

TUI:  AgentSubscribe { agent_id: "child_a" }
      TUI 切换 active_agent = "child_a"，消费 child 的 stream

child 完成：

TUI: ←── AgentDisconnected { agent_id: "child_a", reason: "completed" }
      TUI 标记 agent 为完成，不删除（保留 timeline 可回看）

main 完成：

TUI: ←── AgentDisconnected { agent_id: "main", reason: "completed" }
```

---

## 4. Session 持久化与恢复

### 4.1 存储

```
~/.piko/sessions/<encoded-cwd>/<session-id>.jsonl
```

每行一个 `SessionTreeEntry`。hostd 通过 `persist_from_event(PersistEvent)` 写入。

### 4.2 Resume

```
TUI:  SessionOpen { session_id } ──→ hostd
TUI: ←── CommandResponse { SessionOpened { snapshot } }
      snapshot.entries: Vec<SessionTreeEntry>
      TUI 遍历 entries → 重建 timeline → 渲染

      如果 session 有 active_turn：
        hostd 从 JSONL 读取 transcript + sidecar tasks.json
        → 重建 orchd agent 结构
        → orchd.run() 继续执行
```

### 4.3 SessionTree 导航

```
TUI:  SessionNavigate { target_entry_id }  ──→ hostd
      hostd 移动 current_leaf_id → 返回导航后的 snapshot

TUI: ←── CommandResponse { SessionNavigated { new_leaf_id, editor_text?, summary_entry? } }
      TUI 根据 entry 重建 timeline 分支视图
```

---

## 5. TUI 事件消费

### 5.1 Display 事件 → Timeline

```rust
fn apply_display(timeline: &mut Timeline, event: DisplayEvent) {
    match event {
        TextDelta { message_id, delta, .. } => timeline.append_text_delta(message_id, delta),
        ThinkingDelta { message_id, delta, .. } => timeline.append_thinking_delta(message_id, delta),
        ToolStarted { tool_call_id, tool_name, args, .. } => timeline.upsert_tool(tool_call_id, tool_name, Running, args),
        ToolEnded { tool_call_id, tool_name, result, is_error } => timeline.update_tool(tool_call_id, tool_name, if is_error { Failed } else { Completed }, result),
        Finalized { message_id, content, .. } => timeline.complete_assistant(message_id, content),
        InteractionRequested { .. } => interactions.push(...),
        InteractionResolved { .. } => interactions.resolve(...),
        MessageStart { message_id, role } => timeline.start_message(message_id, role),
        MessageEnd { message_id, stop_reason } => timeline.finish_message(message_id, stop_reason),
        ToolCallDelta { .. } => {} // 当前不渲染增量
    }
}
```

### 5.2 Lifecycle 事件 → 状态

```rust
fn apply_lifecycle(state: &mut AppState, event: LifecycleEvent) {
    match event {
        Task(TaskEvent::Created { task_id, .. }) => {
            state.status = format!("task {} created", short_id(&task_id));
            state.session.active_turn_id = Some(turn_id);
        }
        Task(TaskEvent::Completed { task_id, summary, .. }) => {
            state.status = format!("task {} completed", short_id(&task_id));
        }
        Task(TaskEvent::Failed { error, .. }) => state.push_error(error),
        Turn(TurnEvent::Started { turn_id, root_task_id }) => {
            state.session.active_turn_id = Some(turn_id);
            state.status = format!("turn {turn_id} running ({root_task_id})");
        }
        Turn(TurnEvent::Completed { turn_id }) => {
            state.session.active_turn_id = None;
            state.status = format!("turn {turn_id} completed");
        }
        Turn(TurnEvent::Failed { error, .. }) => state.push_error(error),
        _ => {}
    }
}
```

---

## 6. 当前实现状态

| 特性 | 状态 |
|---|---|
| 单 agent 流式渲染 | ✅ 完成 |
| Display / Lifecycle 分离 | ✅ 完成 |
| PersistEvent → JSONL 直写 | ✅ 完成 |
| TUI 本地渲染用户消息 | ✅ 完成 |
| 审批交互 | ✅ 完成 |
| 用户交互（ask_user_question） | ✅ 完成 |
| 多 agent pub/sub | 🔲 设计中 |
| AgentList / AgentSubscribe | 🔲 设计中 |
| Session resume 多 agent | 🔲 设计中 |
