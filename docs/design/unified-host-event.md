# Unified HostEvent Design — Rust side (hostd + orchd)

## 1. Problem Statement

当前 Rust 侧有 5 层事件类型，6 次序列化/映射，并且 TS↔Rust 边界协议不统一：

```
AgentActor
 ├─ emit_orch(OrchestratorEvent) ──→ JSON Value ──→ listeners HashMap
 ├─ json!({"type":"task_started"}) ──→ JSON Value ──→ listeners HashMap
 └─ json!({"type":"task_transcript_committed"}) ──→ JSON Value ──→ listeners HashMap
      │
      ├─ subscribe() ──→ deserialize HostEvent | OrchestratorEvent ──→ HostEvent ──→ TS host-runtime
      │                                                                   (camelCase, 16种)
      └─ subscribe_orch() ──→ deserialize OrchEvent | HostEvent ──→ OrchEvent ──→ map_orch_event()
                                                                     (snake_case, 12种)     │
                                                                                     RuntimeEvent
                                                                                     (snake_case, 8种)
                                                                                          │
                                                                              apply_runtime_event_to_state()
                                                                                          │
                                                                              host_protocol::HostEvent
                                                                              (kebab-case, 15种) ──→ TUI
```

**核心问题：**
1. AgentActor 里类型信息在 `serde_json::to_value()` 时就丢了，下游靠 try-deserialize 恢复
2. `subscribe` / `subscribe_orch` 两套 API 维护同一份 listeners HashMap
3. `OrchEvent`(12) → `RuntimeEvent`(8) 是近重复的纯映射层
4. `OrchSourcingEvent`(14) 和流式 `HostEvent`(16) 分家，但实际上应该是同一套事件的持久化子集
5. TS 边界有两条协议：pi-compat `HostEvent`(camelCase) 和 hostd wire `host_protocol::HostEvent`(kebab-case)
6. `OrchEvent` 的 `AskUser`、`SubAgent*`、`PlanUpdated` 在 `map_orch_event` 中被丢弃，说明设计膨胀

---

## 2. Target Architecture

```
                              ┌─────────────────────┐
                              │    EventBus          │  typed broadcast channel
                              │  (tokio::broadcast)   │
                              └───┬──────┬──────┬────┘
                                  │      │      │
                    ┌─────────────┘      │      └─────────────┐
                    ▼                    ▼                    ▼
              ┌──────────┐       ┌──────────┐        ┌──────────────┐
              │ Session   │       │ Event    │        │ Wire         │
              │ Manager   │       │ Store    │        │ Projector    │
              │ (state    │       │ (JSONL   │        │ (TS boundary │
              │  rebuild) │       │  append) │        │  projection) │
              └──────────┘       └──────────┘        └──────┬───────┘
                                                            │
                                              ┌─────────────┘
                                              ▼
                                    host_protocol::HostEvent
                                    (unified wire format)
                                              │
                                              ▼
                                         TUI (TS)
```

**核心原则：**
- **一种事件类型**：整个 Rust 侧只用一个 `HostEvent` enum
- **类型化管道**：`tokio::sync::broadcast` 直接发送 `HostEvent`，没有 JSON 序列化
- **Event sourcing**：所有 Domain 事件追加到 JSONL journal，State 通过 replay 重建
- **Streaming vs Domain**：同一个 enum 中通过 marker 区分是否持久化
- **Wire 投影**：从统一的 `HostEvent` 中提取 TUI 需要的子集，一次映射到 wire format

---

## 3. Unified HostEvent

### 3.1 枚举定义

```rust
/// The single event type for the entire hostd+orchd system.
///
/// Every state change flows through this type. Events are immutable.
///
/// # Categories
/// - **Domain events** (tagged `#[sourcing]`): persisted, used for state reconstruction.
/// - **Streaming events** (not tagged): real-time streaming progress, NOT persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostEvent {
    // ═══════════════════════════════════════════════════════════
    // Run lifecycle (domain)
    // ═══════════════════════════════════════════════════════════
    #[sourcing]
    RunStarted {
        run_id: String,
        agent_id: String,
        timestamp: i64,
    },
    #[sourcing]
    RunCompleted {
        run_id: String,
        agent_id: String,
        status: RunStatus,
        total_steps: u32,
        timestamp: i64,
    },
    #[sourcing]
    RunFailed {
        run_id: String,
        agent_id: String,
        error: String,
        timestamp: i64,
    },
    #[sourcing]
    RunCancelled {
        run_id: String,
        agent_id: String,
        reason: Option<String>,
        timestamp: i64,
    },

    // ═══════════════════════════════════════════════════════════
    // Turn lifecycle (domain)
    // ═══════════════════════════════════════════════════════════
    #[sourcing]
    TurnStarted {
        run_id: String,
        agent_id: String,
        turn_index: u32,
        user_message_id: Option<String>,
    },
    #[sourcing]
    TurnCompleted {
        run_id: String,
        agent_id: String,
        turn_index: u32,
        timestamp: i64,
    },
    #[sourcing]
    TurnFailed {
        run_id: String,
        agent_id: String,
        turn_index: u32,
        error: String,
        timestamp: i64,
    },
    #[sourcing]
    TurnCancelled {
        run_id: String,
        agent_id: String,
        turn_index: u32,
        timestamp: i64,
    },

    // ═══════════════════════════════════════════════════════════
    // Message streaming (streaming — NOT persisted)
    // ═══════════════════════════════════════════════════════════
    MessageStart {
        message_id: String,
        role: MessageRole,
    },
    TextDelta {
        message_id: String,
        delta: String,
    },
    ThinkingDelta {
        message_id: String,
        delta: String,
    },
    MessageEnd {
        message_id: String,
        stop_reason: Option<String>,
    },

    // ═══════════════════════════════════════════════════════════
    // Tool execution (domain)
    // ═══════════════════════════════════════════════════════════
    #[sourcing]
    ToolStart {
        tool_call_id: String,
        tool_name: String,
        args: serde_json::Value,
        parent_message_id: Option<String>,
        timestamp: i64,
    },
    #[sourcing]
    ToolEnd {
        tool_call_id: String,
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
        timestamp: i64,
    },

    // ═══════════════════════════════════════════════════════════
    // Approval (domain)
    // ═══════════════════════════════════════════════════════════
    #[sourcing]
    ApprovalRequested {
        approval_id: String,
        tool_name: String,
        tool_args: serde_json::Value,
        timestamp: i64,
    },
    #[sourcing]
    ApprovalResolved {
        approval_id: String,
        decision: ApprovalDecision,
        timestamp: i64,
    },

    // ═══════════════════════════════════════════════════════════
    // Agent management (domain)
    // ═══════════════════════════════════════════════════════════
    #[sourcing]
    AgentRegistered {
        agent_id: String,
        name: String,
        role: String,
        system_prompt: String,
        tool_set_ids: Vec<String>,
        timestamp: i64,
    },
    #[sourcing]
    AgentUnregistered {
        agent_id: String,
        timestamp: i64,
    },

    // ═══════════════════════════════════════════════════════════
    // Model config (domain)
    // ═══════════════════════════════════════════════════════════
    #[sourcing]
    ModelConfigChanged {
        model_id: String,
        provider: String,
        timestamp: i64,
    },

    // ═══════════════════════════════════════════════════════════
    // Queue (domain)
    // ═══════════════════════════════════════════════════════════
    #[sourcing]
    QueueUpdate {
        agent_id: String,
        steer_count: u32,
        follow_up_count: u32,
        next_turn_count: u32,
        steer_preview: Option<String>,
        follow_up_preview: Option<String>,
    },

    // ═══════════════════════════════════════════════════════════
    // Plan (domain)
    // ═══════════════════════════════════════════════════════════
    #[sourcing]
    PlanUpdated {
        agent_id: String,
        plan: Vec<serde_json::Value>,
        timestamp: i64,
    },

    // ═══════════════════════════════════════════════════════════
    // Session (domain — hostd level)
    // ═══════════════════════════════════════════════════════════
    #[sourcing]
    SessionCreated {
        session_id: String,
        cwd: String,
        timestamp: i64,
    },
    #[sourcing]
    UserMessageSubmitted {
        session_id: String,
        message_id: String,
        text: String,
        timestamp: i64,
    },
}

// ── Companion types ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Completed,
    Aborted,
    Error,
    ContextOverflow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    ToolResult,
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

### 3.2 变体总数：21 种

| 类别 | 变体 | 持久化 |
|---|---|---|
| Run lifecycle | 4 (RunStarted/Completed/Failed/Cancelled) | ✓ |
| Turn lifecycle | 4 (TurnStarted/Completed/Failed/Cancelled) | ✓ |
| Message streaming | 4 (MessageStart/TextDelta/ThinkingDelta/MessageEnd) | ✗ |
| Tool | 2 (ToolStart/ToolEnd) | ✓ |
| Approval | 2 | ✓ |
| Agent | 2 | ✓ |
| Model | 1 | ✓ |
| Queue | 1 | ✓ |
| Plan | 1 | ✓ |
| Session | 2 | ✓ |

对比现状：
- 删除了：`OrchestratorEvent`(14), `HostEvent`(16, pi-compat Rust), `OrchEvent`(12), `RuntimeEvent`(8), `OrchSourcingEvent`(14)
- 合并为：**1 个** `HostEvent`(21)
- Streaming events (4) 不持久化，Domain events (17) 持久化

---

## 4. EventBus

### 4.1 核心结构

```rust
use tokio::sync::broadcast;

/// Central event bus. All components publish to and subscribe from this.
pub struct EventBus {
    tx: broadcast::Sender<Arc<HostEvent>>,
    /// Persistence writer (for domain events only).
    journal: EventJournal,
}

impl EventBus {
    pub fn new(capacity: usize, journal: EventJournal) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx, journal }
    }

    /// Publish an event to all subscribers.
    /// Domain events are also written to the journal.
    pub fn publish(&self, event: HostEvent) {
        if event.is_sourcing() {
            // Non-blocking: spawn so slow journal writes don't stall the bus
            let journal = self.journal.clone();
            let ev = event.clone();
            tokio::spawn(async move { journal.append(&ev).await });
        }

        let arc = Arc::new(event);
        let _ = self.tx.send(arc); // ignore NoReceiver errors
    }

    /// Subscribe to all events.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<HostEvent>> {
        self.tx.subscribe()
    }
}
```

### 4.2 关键设计决策

| 决策 | 原因 |
|---|---|
| `broadcast` 而非 `mpsc` | 多个订阅者，每个都能收到所有事件 |
| `Arc<HostEvent>` | 避免克隆大对象（如 `args` JSON、`plan` 数组） |
| Journal 写入异步 spawn | 不阻塞事件总线 |
| Streaming events 不写 journal | 避免 TextDelta 泛滥 JSONL 文件 |
| 单一 `publish` 方法 | 不需要区分 "emit to listeners" vs "emit sourcing" |

### 4.3 HostEvent 的 `is_sourcing()` 辅助方法

```rust
impl HostEvent {
    /// Whether this event should be persisted in the event journal.
    /// Streaming events (TextDelta, ThinkingDelta) return false.
    pub fn is_sourcing(&self) -> bool {
        !matches!(
            self,
            HostEvent::TextDelta { .. }
                | HostEvent::ThinkingDelta { .. }
                | HostEvent::MessageStart { .. }
                | HostEvent::MessageEnd { .. }
        )
    }
}
```

---

## 5. 组件集成

### 5.1 hostd 主结构

```rust
pub struct Hostd {
    event_bus: EventBus,
    session_manager: SessionManager,
    agent_runner: AgentRunner,
    // ...
}

impl Hostd {
    pub async fn submit_prompt(&self, session_id: &str, text: &str) {
        let message_id = Uuid::new_v4().to_string();

        // Publish domain event
        self.event_bus.publish(HostEvent::UserMessageSubmitted {
            session_id: session_id.to_string(),
            message_id: message_id.clone(),
            text: text.to_string(),
            timestamp: now_ms(),
        });

        // Start agent run
        let run_id = Uuid::new_v4().to_string();
        self.event_bus.publish(HostEvent::RunStarted {
            run_id: run_id.clone(),
            agent_id: "main".to_string(),
            timestamp: now_ms(),
        });

        // Run the agent loop (internally calls orchd model/tool execution)
        self.agent_runner.run(run_id, session_id, text).await;
    }
}
```

### 5.2 AgentRunner（内部调用 orchd）

```rust
pub struct AgentRunner {
    event_bus: EventBus,
    model_executor: Arc<dyn ModelStepExecutor>,
    tool_registry: Arc<ToolRegistry>,
}

impl AgentRunner {
    pub async fn run(&self, run_id: String, session_id: &str, prompt: &str) {
        let mut transcript: Vec<Message> = vec![];
        let mut turn_index: u32 = 0;

        // Publish turn start
        self.event_bus.publish(HostEvent::TurnStarted {
            run_id: run_id.clone(),
            agent_id: "main".to_string(),
            turn_index,
            user_message_id: None,
        });

        loop {
            // ── Call model (orchd) ──
            let step = self.model_executor
                .execute_step(ModelStepInput { transcript: transcript.clone(), .. })
                .await;

            // ── Stream deltas via EventBus ──
            let msg_id = format!("msg_{}", Uuid::new_v4());
            self.event_bus.publish(HostEvent::MessageStart {
                message_id: msg_id.clone(),
                role: MessageRole::Assistant,
            });
            for delta in step.deltas {
                self.event_bus.publish(HostEvent::TextDelta {
                    message_id: msg_id.clone(),
                    delta,
                });
            }
            self.event_bus.publish(HostEvent::MessageEnd {
                message_id: msg_id.clone(),
                stop_reason: step.stop_reason.clone(),
            });

            transcript.push(step.assistant_message);

            // ── Execute tools (orchd) ──
            for tc in &step.tool_calls {
                let tool_id = tc.id.clone();
                self.event_bus.publish(HostEvent::ToolStart {
                    tool_call_id: tool_id.clone(),
                    tool_name: tc.name.clone(),
                    args: tc.arguments.clone(),
                    parent_message_id: Some(msg_id.clone()),
                    timestamp: now_ms(),
                });

                let result = self.tool_registry.execute(tc).await;
                self.event_bus.publish(HostEvent::ToolEnd {
                    tool_call_id: tool_id.clone(),
                    tool_name: tc.name.clone(),
                    result: result.output.clone(),
                    is_error: !result.ok,
                    timestamp: now_ms(),
                });

                transcript.push(result.tool_message);
            }

            // Check terminal
            if step.stop_reason.as_deref() == Some("end_turn") {
                break;
            }

            turn_index += 1;
            self.event_bus.publish(HostEvent::TurnCompleted {
                run_id: run_id.clone(),
                agent_id: "main".to_string(),
                turn_index: turn_index - 1,
                timestamp: now_ms(),
            });
            if !is_terminal {
                self.event_bus.publish(HostEvent::TurnStarted {
                    run_id: run_id.clone(),
                    agent_id: "main".to_string(),
                    turn_index,
                    user_message_id: None,
                });
            }
        }

        // Publish run end
        self.event_bus.publish(HostEvent::RunCompleted {
            run_id: run_id.clone(),
            agent_id: "main".to_string(),
            status: RunStatus::Completed,
            total_steps: turn_index + 1,
            timestamp: now_ms(),
        });
    }
}
```

### 5.3 关键变化：不再有 AgentActor / ActorSystem

当前 orchd 用 tokio-actors 管理 AgentActor 的生命周期。新设计中 AgentRunner 直接持有 model_executor 和 tool_registry，在 hostd 的 async task 中同步执行 agent loop。

**优点：**
- 消除了 `ActorSystem`、`AgentMsg`、`AgentActor` 等间接层
- 事件直接在 AgentRunner 中发布，无需 `emit_fn(agent_id, JSON Value)`
- hostd 对 agent 生命周期有完全控制权（取消 = drop Future + CancellationToken）

---

## 6. Event Sourcing & State Rebuild

### 6.1 EventJournal

```rust
/// Append-only JSONL event journal.
pub struct EventJournal {
    writer: Arc<Mutex<BufWriter<File>>>,
}

impl EventJournal {
    pub async fn append(&self, event: &HostEvent) -> Result<()> {
        let mut w = self.writer.lock().await;
        serde_json::to_writer(&mut *w, event)?;
        w.write_all(b"\n")?;
        w.flush()?;
        Ok(())
    }
}
```

### 6.2 State Rebuild

```rust
/// Rebuild session state by replaying sourcing events.
pub fn rebuild_state(events: &[HostEvent]) -> SessionState {
    let mut state = SessionState::default();
    for event in events {
        if !event.is_sourcing() {
            continue; // skip streaming events
        }
        match event {
            HostEvent::SessionCreated { session_id, cwd, .. } => {
                state.session_id = session_id.clone();
                state.cwd = cwd.clone();
            }
            HostEvent::UserMessageSubmitted { message_id, text, .. } => {
                state.messages.push(Message { id: message_id.clone(), text: text.clone(), .. });
            }
            HostEvent::RunStarted { run_id, .. } => {
                state.active_run_id = Some(run_id.clone());
            }
            HostEvent::RunCompleted { run_id, .. } | RunFailed { run_id, .. } => {
                if state.active_run_id.as_deref() == Some(run_id) {
                    state.active_run_id = None;
                }
            }
            HostEvent::ToolStart { tool_call_id, tool_name, args, parent_message_id, .. } => {
                state.active_tools.insert(tool_call_id.clone(), ToolState { .. });
            }
            HostEvent::ToolEnd { tool_call_id, result, is_error, .. } => {
                state.active_tools.remove(tool_call_id);
            }
            HostEvent::MessageStart { .. }
            | HostEvent::TextDelta { .. }
            | HostEvent::ThinkingDelta { .. }
            | HostEvent::MessageEnd { .. } => {
                // Streaming events — never reach here due to is_sourcing() guard
                unreachable!()
            }
        }
    }
    state
}
```

### 6.3 与当前 OrchSourcingEvent 的对比

当前 `OrchSourcingEvent`（14 种）完全被统一的 `HostEvent` 吸收。`apply_event()` 和 `rebuild_state()` 逻辑不变，但输入从分离的 `OrchSourcingEvent` 变为统一的 `HostEvent` 子集。

---

## 7. Wire Protocol（TS↔Rust 边界）

### 7.1 设计

统一的 `HostEvent` 直接作为 wire format。TS 侧接收完整的 `HostEvent` JSON，在 TUI 层按需消费。

**serde 配置：** 使用 `rename_all = "snake_case"` + 按需 `#[serde(rename = "...")]` 保持与 TS 侧的兼容性。

TS 侧对应定义：
```typescript
// packages/host-tui/src/client/hostd-protocol.ts
export type HostEvent =
  | { type: "run_started"; run_id: string; agent_id: string; timestamp: number }
  | { type: "run_completed"; run_id: string; agent_id: string; status: RunStatus; ... }
  | { type: "text_delta"; message_id: string; delta: string }
  | { type: "thinking_delta"; message_id: string; delta: string }
  | { type: "tool_start"; tool_call_id: string; tool_name: string; args: unknown; ... }
  | { type: "tool_end"; tool_call_id: string; tool_name: string; result: unknown; ... }
  | { type: "approval_requested"; approval_id: string; tool_name: string; ... }
  | { type: "approval_resolved"; approval_id: string; decision: string; ... }
  | { type: "queue_update"; agent_id: string; steer_count: number; ... }
  | { type: "turn_started"; ... }
  | { type: "turn_completed"; ... }
  | { type: "session_created"; session_id: string; cwd: string; ... }
  // ... 共 21 种
```

### 7.2 hostEventToTuiEvents 简化

TS 侧的映射变得 trivial，因为 wire 事件和 TuiEvent 已经高度一致：

```typescript
function hostEventToTuiEvent(event: HostEvent): TuiEvent | null {
    switch (event.type) {
        case "text_delta":
            return { type: "assistant_delta", delta: event.delta };
        case "thinking_delta":
            return { type: "thinking_delta", delta: event.delta };
        case "tool_start":
            return { type: "tool_call_started", id: event.tool_call_id, name: event.tool_name, ... };
        case "tool_end":
            return { type: "tool_call_ended", id: event.tool_call_id, ... };
        case "run_started":
            return { type: "stream_started" };
        case "run_completed":
            return { type: "stream_settled" };
        case "turn_failed":
            return { type: "turn_failed", error: event.error };
        // ... 其余为 1:1 映射
    }
}
```

---

## 8. 删除清单

实施此设计后，以下内容应删除：

| 文件/类型 | 原因 |
|---|---|
| `orchd/src/protocol/events.rs` — `OrchEvent` | 合并到统一 `HostEvent` |
| `orchd/src/protocol/events.rs` — `OrchestratorEvent` | AgentActor 不再存在，直接 emit HostEvent |
| `orchd/src/protocol/events.rs` — pi-compat `HostEvent` | 旧 pi 协议，被统一 `HostEvent` 替代 |
| `orchd/src/protocol/event_store.rs` — `OrchSourcingEvent` | 被统一 `HostEvent` + `is_sourcing()` 替代 |
| `orchd/src/runtime/events.rs` — `RuntimeEvent` | 不再需要中间映射层 |
| `orchd/src/runtime/facade.rs` — `OrchestratorRuntime` trait | hostd 直接调用 orchd 组件，不需要 trait 抽象 |
| `orchd/src/runtime/core_runtime.rs` — `CoreOrchestratorRuntime` | 同上 |
| `orchd/src/runtime/types.rs` — `RuntimeConfig`, `RunTurnInput` 等 | 简化或合并到 hostd 配置 |
| `orchd/src/actors/agent/` — 整个目录 | AgentActor/ActorSystem 不再需要 |
| `orchd/src/orchestrator/state.rs` — `subscribe()`, `subscribe_orch()` | 被 EventBus::subscribe() 替代 |
| `orchd/src/protocol/orch_runtime.rs` — `OrchRuntime` trait | hostd 不再需要 trait 抽象 |
| `orchd/src/protocol/runtime.rs` — 旧的 `OrchRunResult` 等 | 被统一 HostEvent 的 RunCompleted 携带 |
| `host-runtime/src/orchd/protocol/events.ts` — `HostEvent`(16种) | TS 侧迁移到新的 wire HostEvent |
| `host-runtime/src/orchd/protocol/runtime-stream.ts` — `HostRuntimeEvent` | 不再需要投影层 |
| `host-tui/src/client/hostd-events.ts` — `hostEventToTuiEvents` | 重写为简化版映射 |

---

## 9. 迁移路径

### Phase 1: 新建统一 HostEvent + EventBus

1. 在 `orchd/src/protocol/` 新建 `host_event.rs` — 统一 `HostEvent` enum
2. 新建 `hostd/src/event_bus.rs` — `EventBus` + `EventJournal`
3. 保留旧代码不动，新事件系统与旧系统并行运行

### Phase 2: hostd 接入 EventBus

4. `hostd/src/server.rs` 中的 turn runner 改为通过 EventBus 发布事件
5. `hostd/src/turn_runner.rs` 的 `OrchTurnRunner` 改为新的 `AgentRunner`
6. TUI wire format 切换到新的统一 `HostEvent`

### Phase 3: 删除旧代码

7. 验证 TS 侧 TUI 在新 wire format 下正常工作
8. 删除 `OrchEvent`, `RuntimeEvent`, `OrchestratorEvent`, `OrchSourcingEvent`
9. 删除 `AgentActor`, `subscribe()`, `subscribe_orch()`, `OrchRuntime` trait
10. 删除 TS 侧 `HostEvent`(pi-compat), `HostRuntimeEvent`, `projectHostEvent()`

### Phase 4: 清理 TS 侧

11. `host-runtime` 不再直接订阅 orchd — 只通过 hostd wire 协议交互
12. 移除 `host-runtime/src/orchd/protocol/events.ts` 中的旧 HostEvent

---

## 10. 设计收益

| 指标 | 现状 | 新设计 |
|---|---|---|
| Rust 侧事件类型数量 | 5 (`OrchestratorEvent`, `HostEvent`, `OrchEvent`, `RuntimeEvent`, `OrchSourcingEvent`) | 1 (`HostEvent`) |
| 跨进程 JSON 序列化次数 | 3-4 次 | 0 次（内部走 typed broadcast） |
| 订阅 API 数量 | 2 (`subscribe` + `subscribe_orch`) | 1 (`EventBus::subscribe()`) |
| Wire 协议种类 | 2 (pi HostEvent camelCase + hostd HostEvent kebab-case) | 1 (unified HostEvent) |
| Agent 实现 | ActorSystem + AgentActor + AgentMsg + AgentWorkerState | 单 async fn AgentRunner::run() |
| 事件 sourcing 持久化 | 分离的 `OrchSourcingEvent` | 统一 `HostEvent` 子集 + `is_sourcing()` |
| TS 侧事件映射 | `HostEvent → HostRuntimeEvent → TuiEvent` 两次映射 | `HostEvent → TuiEvent` 一次映射 |
