# Stream Architecture

piko 的 stream 架构定义 LLM 原始输出如何流经各处理层，从 HTTP SSE 到 TUI 渲染和持久化落盘。

---

## 1. 概述

piko 以 agent 为执行单元。每个 agent 拥有独立的完整 stream pipeline，不跨 agent 共享或合并。

三层处理模型：

```
Layer 1: llmd         忠实转发 LLM 原始输出，不做业务逻辑
Layer 2: dispatcher   按 GatewayEvent 类型分流到 3 个 consumer channel
Layer 3: consumers    persist（落盘）、display（TUI 渲染）、tool executor（工具执行）
```

---

## 2. 单 Agent 数据流

```mermaid
graph TD
  LLM[LLM raw HTTP SSE]
  GENAI[genai - rust lib]
  LLMD[llmd executor<br/>pass-through]

  subgraph GW[GatewayEvent stream]
    GW_T[ContentDelta]
    GW_TH[ReasoningDelta]
    GW_TC[ToolCallChunk]
    GW_D[Done]
  end

  subgraph DISP[dispatcher - typed fanout]
    DISP_CORE[dispatch by type]
  end

  subgraph PERSIST[persist channel]
    P_FINAL[Finalized]
    P_TCR[ToolResultCommitted]
    P_HOST[hostd]
  end

  subgraph DISPLAY[display channel]
    D_TEXT[TextDelta]
    D_THINK[ThinkingDelta]
    D_FINAL[Finalized]
    D_TS[ToolEvent::Start]
    D_TE[ToolEvent::End]
    D_INT[InteractionEvent]
    D_TUI[TUI render]
  end

  subgraph EXEC[tool executor]
    EXEC_AGG[aggregate ToolCallChunks]
    EXEC_RUN[execute tools]
    EXEC_RESULT[results]
  end

  LLM --> GENAI --> LLMD
  LLMD --> GW_T & GW_TH & GW_TC & GW_D
  GW_T & GW_TH & GW_TC & GW_D --> DISP_CORE

  DISP_CORE -->|ContentDelta| D_TEXT
  DISP_CORE -->|ReasoningDelta| D_THINK
  DISP_CORE -->|Done triggers| P_FINAL
  DISP_CORE -->|Done triggers| D_FINAL
  DISP_CORE -->|ToolCallChunk| EXEC_AGG

  D_TEXT & D_THINK & D_FINAL & D_TS & D_TE & D_INT --> D_TUI
  P_FINAL & P_TCR --> P_HOST

  EXEC_AGG --> EXEC_RUN
  EXEC_RUN --> EXEC_RESULT
  EXEC_RESULT -->|ToolEvent::Start| D_TS
  EXEC_RESULT -->|ToolEvent::End| D_TE
  EXEC_RESULT -->|ToolEvent::End| P_TCR

  style DISP_CORE fill:#d4edda,stroke:#28a745
  style EXEC_AGG fill:#fff3cd,stroke:#ffc107
  style P_FINAL fill:#cce5ff,stroke:#004085
  style D_FINAL fill:#cce5ff,stroke:#004085
```

### 各层职责

| 层 | 组件 | 职责 | 输入 | 输出 |
|---|---|---|---|---|
| 1 | llmd executor | 调用 genai，映射 LLM 响应为 GatewayEvent stream | genai ChatStreamEvent | GatewayEvent stream |
| 2 | dispatcher | 按事件类型分流到对应 consumer channel | GatewayEvent | persist channel + display channel + tool executor |
| 3a | persist channel | 接收最终态事件，转换为 SessionTreeEntry，写入 JSONL | PersistEvent | 文件 I/O |
| 3b | display channel | 接收所有显示相关事件，渲染 TUI timeline | DisplayEvent | TUI frame |
| 3c | tool executor | 聚合 ToolCallChunk → 完整 ToolCall → 执行 → 回投结果 | ToolCallChunk stream | ToolEvent（投递 persist + display） |

### 关键约束

- **llmd 不做聚合**：ToolCallChunk 为 LLM 原始输出，聚合由 tool executor 负责。
- **类型决定路由**：ContentDelta 只走 display channel。persist channel 不受理中间态事件。
- **Arc 共享**：Finalized 和 ToolResultCommitted 同时投递 persist + display，用 Arc 避免深拷贝。
- **tool executor 直接回投**：执行完成后不经过 dispatcher，直接向 persist 和 display channel 发送结果。

---

## 3. 多 Agent 架构

### 3.1 原则

- 每个 agent 拥有独立的完整 pipeline
- 不同 agent 的 stream 不合并
- orchd supervisor 统一管理 agent 生命周期
- spawn 语义通过 orchd AgentSpawner 实现

### 3.2 架构图

```mermaid
graph TD
  subgraph ROOT[Root Agent]
    R_LLMD[llmd]
    R_DISP[dispatcher]
    R_PERSIST[persist channel]
    R_DISPLAY[display channel]
    R_EXEC[tool executor]
  end

  subgraph CHILD[Child Agent]
    C_LLMD[llmd]
    C_DISP[dispatcher]
    C_PERSIST[persist channel]
    C_DISPLAY[display channel]
    C_EXEC[tool executor]
  end

  subgraph ORCHD[orchd supervisor]
    REG[agent registry]
    SPAWNER[AgentSpawner]
  end

  subgraph TUI[TUI 内存]
    CHANNELS[display_channels: HashMap&lt;AgentId, DisplayStream&gt;]
    ACTIVE[active_agent]
    RENDER[render active]
  end

  subgraph HOSTD[hostd]
    JSONL[session JSONL]
    SIDECAR[tasks.json]
  end

  R_EXEC -->|spawn| SPAWNER
  SPAWNER -->|register| REG
  SPAWNER -.->|create| C_LLMD
  R_EXEC -->|subscribe Done| C_DISP

  R_PERSIST --> JSONL
  C_PERSIST --> JSONL
  REG --> SIDECAR

  R_DISPLAY --> CHANNELS
  C_DISPLAY --> CHANNELS
  CHANNELS --> ACTIVE
  ACTIVE --> RENDER

  style REG fill:#d4edda,stroke:#28a745
  style SPAWNER fill:#d4edda,stroke:#28a745
  style CHANNELS fill:#cce5ff,stroke:#004085
```

### 3.3 Orchd Supervisor

supervisor 管理两类 dispatch instance：

- **AgentDispatch**（per-agent）：每个 agent 一个，消费 LLM GatewayEvent stream，产出到 session channels
- **LifecycleDispatch**（per-session）：消费 supervisor 的编排事件（TaskEvent / TurnEvent），产出到 session channels

两类 instance 共享同一个 session channel pair，通过 dispatch framework 统一管理。

- **agent registry**：维护 `HashMap<TaskId, AgentHandle>`，追踪活跃 agent。
- **AgentSpawner**：接收 root agent 的 spawn tool call，创建新的 AgentDispatch 实例。
- **生命周期管理**：child agent 完成时清理资源，通知订阅了 Done 的父 agent。

### 3.4 Spawn 语义

| 语义 | 父 agent 行为 | 子 agent 行为 | 父 agent 获取结果 |
|---|---|---|---|
| `spawn_detached` | tool executor 调用 AgentSpawner，立即返回 task_id | 独立运行完整 pipeline | 后续轮次调用 `poll_task(task_id)` |
| `spawn` | 同 `spawn_detached`，并订阅 child 的 Done event | 同 `spawn_detached` | 通过订阅 Done 作为 tool result 返回 |

`spawn` 本质为 `spawn_detached` + 父 agent 注册为子 agent 的 Done consumer。父 agent 通过 tool result 获取结果，agent_loop 流程不变。

---

## 4. TUI View 切换

TUI 在内存中维护所有 agent 的 display channel：

```rust
struct TuiState {
    active_agent: AgentId,
    display_channels: HashMap<AgentId, DisplayStream>,
}

fn render(&mut self) {
    let channel = &mut self.display_channels[&self.active_agent];
    // 消费 channel 事件 → 更新 timeline → 渲染
}

fn switch_agent(&mut self, agent_id: AgentId) {
    self.active_agent = agent_id;
}
```

- 所有 display channel 持续消费，切换时不暂停
- 切换不 reload JSONL，不 reconcile timeline
- 默认显示 root agent

---

## 5. Task Tree Sidecar

Agent 父子关系由 sidecar 文件维护，不写入 JSONL：

**路径**：`<session-dir>/tasks.json`

```json
{
  "task_root_001": {
    "parent_task_id": null,
    "agent_id": "main",
    "status": "running"
  },
  "task_child_a_001": {
    "parent_task_id": "task_root_001",
    "agent_id": "child_a",
    "status": "completed"
  }
}
```

**hostd 管理**：
- `turn_submit` → 创建 root task entry
- spawn tool call → 创建 child task entry
- agent 完成 → 更新 status
- resume → 通过 sidecar 重建 agent 结构
