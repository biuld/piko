# piko 项目现状

## 1. 架构

```
TUI (TypeScript, host-tui)  ←→  hostd (Rust, hostd)  ←→  orchd (Rust, orchd)
       │                              │                        │
       └── JSON-lines ────────────────┘                        │
       (HostEvent, snake_case, 21 variants)                    │
                                                               │
                                              hostd ──OrchCore (direct link)──→ orchd
                                                               (OrchEvent, 12 variants)
```

### 组件

| 包 | 语言 | 角色 |
|---|------|------|
| `host-tui` | TypeScript | 终端 UI，通过 JSON-lines 与 hostd 通信 |
| `hostd` | Rust | 宿主守护进程：session 管理、turn 执行、事件分发 |
| `orchd` | Rust（纯库）| 编排引擎：agent loop、工具执行、模型调用 |
| `host-runtime` | TypeScript | **废弃中**，将被 hostd 取代 |

---

## 2. 事件协议

### 统一 HostEvent（hostd ↔ TUI）

单一事件类型，21 个变体，snake_case。定义位置：

| 位置 | 语言 |
|------|------|
| `hostd/src/api/mod.rs` | Rust（权威定义） |
| `host-tui/src/client/hostd-protocol.ts` | TypeScript（镜像） |

```
Domain events (15):            Streaming events (8):
  user_message_submitted         message_start
  assistant_message_completed    message_end
  tool_result_committed          text_delta
  turn_started                   thinking_delta
  turn_completed                 tool_start
  turn_failed                    tool_end
  turn_cancelled                 approval_requested
  task_created                   approval_resolved
  task_started
  task_completed
  task_failed
  task_cancelled
  task_transcript_committed
  task_joined
  task_steered
  session_created
  queue_update
  model_config_changed
```

### OrchEvent（orchd → hostd，内部）

12 个变体，task 级别，无 session 上下文。hostd 通过 `map_orch_to_host_event()` 将其包装为 HostEvent（加上 `session_id`、`timestamp`）。

```
MessageStart, TextDelta, ThinkingDelta, MessageEnd,
ToolStart, ToolEnd,
AskUser, RequestApproval,
SubAgentSpawned, SubAgentCompleted,
PlanUpdated,
TaskError, TaskEnd
```

### 已删除的旧事件类型

| 类型 | 原位置 | 变体数 |
|------|--------|--------|
| `HostEvent`（kebab-case）| `orchd/src/protocol/events.rs` | 16 |
| `OrchestratorEvent` | `orchd/src/protocol/events.rs` | ~13 |
| `HostLifecycleEvent` | `host-runtime/src/host/lifecycle/` | 13 |
| `HostRuntimeEvent` | `host-runtime/src/orchd/protocol/runtime-stream.ts` | 11 |
| `HostEvent`（dot-notation）| `host-tui/src/client/hostd-protocol.ts` | 16 |
| `OrchRuntime` trait | `orchd/src/protocol/orch_runtime.rs` | — |
| `CoreOrchestratorRuntime` | `orchd/src/runtime/` | — |

**结果**：5 层 64+ 变体 → 2 层（OrchEvent + HostEvent），33 变体。

---

## 3. 构建状态

| 包 | 语言 | 构建 | 测试 |
|---|------|------|------|
| `orchd` | Rust | ✅ | ✅ 0 fail |
| `hostd` | Rust | ✅ | ✅ 0 fail |
| `host-runtime` | TS | ✅ | ✅ 176 pass, 0 fail |
| `host-tui` | TS | ✅ | ✅ 207 pass（4 个预先存在的 JSX 问题） |

---

## 4. 功能对照：hostd vs host-runtime

### ✅ 已完成

| 功能 | hostd | host-runtime |
|------|-------|-------------|
| 基本 session 创建/打开/删除 | ✅ | ✅ |
| Turn 提交 + 取消 | ✅ | ✅ |
| 流式事件（text/thinking/tool/approval）| ✅ | ✅ |
| System prompt 构建 | ✅ | ✅ |
| Skills 加载 + prompt template | ✅ | ✅ |
| Model registry + API key auth | ✅ | ✅ |
| JSONL 持久化（存储层）| ✅ | ✅ |

### ❌ hostd 缺失

| 功能 | 影响 | 复杂度 |
|------|------|--------|
| **Session 操作**（list/fork/navigate/import/clone/switch/rename） | 存储层已就绪，需接线到 server 命令 | 低 |
| **队列管理**（steer/follow-up/next-turn） | 不支持注入运行中的 turn | 中 |
| **Compaction 执行** | 只有骨架类型，无 LLM 摘要和自动触发 | 中 |
| **Multi-agent**（spawn sub-task、inter-task steering） | hostd 当前单 agent 单 turn | 高 |
| **OAuth**（device code flow） | 无 OAuth 流程实现 | 中 |
| **MCP 工具集成** | 完全缺失 | 高 |
| **运行时模型/工具切换** | 需重启 | 低 |
| **Session 元数据条目**（model/thinking/compaction 日志） | JSONL 中无此类条目 | 低 |

### ✅ 不需要移植

| 功能 | 原因 |
|------|------|
| `OrchdRpcClient` | hostd 直接 link orchd，不需要 JSON-RPC |
| `EventStream` / `projectHostEvent` | hostd 使用 channel + 直接事件映射 |
| `PikoHost.create()` | 已被 `HostServer` 取代 |
| HTML export | daemon 不需要 |
| 进程内 TUI 集成 | TUI 通过 JSON-lines 连接 |
| `discoverResources()` | hostd 单独加载每种资源 |

---

## 5. 删除清单

### 已删除

| 删除项 | 位置 |
|--------|------|
| `packages/orchd/src/runtime/` | 5 文件，OrchRuntime trait + CoreOrchestratorRuntime |
| `packages/orchd/src/protocol/orch_runtime.rs` | OrchRuntime trait |
| `packages/orchd/src/rpc/` | 4 文件，JSON-RPC 层 |
| `packages/orchd/src/tools/rpc_host_provider.rs` | RPC host tool provider |
| `packages/orchd/src/main.rs` | 独立二进制入口 |
| `packages/orchd/Cargo.toml` `[[bin]]` | 二进制目标 |
| `packages/host-protocol/` | 独立 Rust crate，合并入 `hostd/src/api/mod.rs` |
| `packages/host-runtime/src/host/lifecycle/` | 2 文件，HostLifecycleEvent |
| `HostRuntimeEvent` | 从 `runtime-stream.ts` 移除 |
| `projectHostEvent()` | 从 `run/controller.ts` 移除 |
| 旧 `HostEvent`（dot-notation）| `host-tui/src/client/hostd-protocol.ts` |
| 旧 `HostEvent`（kebab-case, Rust）| `orchd/src/protocol/events.rs` |
| `OrchestratorEvent`（Rust）| `orchd/src/protocol/events.rs` |
| `OrchRuntime` trait（Rust）| `orchd/src/protocol/orch_runtime.rs` |
| `subscribe()`（旧版）| `orchd/src/orchestrator/state.rs` + `core.rs` |

### 待删除

| 删除项 | 阻塞原因 |
|--------|---------|
| `packages/host-runtime/`（整个包） | hostd 功能缺口（见 §4） |
| `orchd/src/actors/agent/`（AgentActor） | 需 hostd 直接管理 agent loop |
| TS `OrchWireEvent`（`events.ts`） | 随 host-runtime 一起删 |

---

## 6. 目录结构

```
packages/
├── hostd/                    # Rust — 宿主守护进程
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── api/mod.rs        # 协议类型（HostEvent, HostCommand, CommandAck）
│       ├── auth/             # API key + OAuth
│       ├── compaction/       # Token 估算 + 设置（执行待完成）
│       ├── models/           # Model registry + provider 配置
│       ├── prompts/          # System prompt + 模板展开 + 上下文文件
│       ├── server.rs         # JSON-lines 服务器
│       ├── session/          # JSONL 存储仓库
│       ├── settings/         # 分层设置（全局 → 项目 → CLI）
│       ├── skills/           # Skill 加载
│       ├── state.rs          # Session 状态
│       └── turn_runner.rs    # Turn 执行（Mock + Orch）
│
├── orchd/                    # Rust — 编排引擎（纯库）
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── actors/agent/     # Agent actor（agent loop + 工具执行）
│       ├── model/            # LLM executor
│       ├── orchestrator/     # OrchCore（任务 spawn、事件订阅、状态快照）
│       ├── protocol/         # 协议类型（OrchEvent、agents、tools、events）
│       └── tools/            # Tool registry + providers
│
├── host-tui/                 # TypeScript — 终端 UI
│   └── src/
│       └── client/
│           ├── hostd-protocol.ts  # HostEvent + HostCommand + CommandAck
│           ├── hostd-client.ts    # JSON-lines transport
│           └── hostd-events.ts    # HostEvent → TuiEvent 映射
│
├── host-runtime/             # TypeScript — 废弃中
│   └── src/
│       ├── host/             # PikoHost（session、run、queue、persistence）
│       ├── orchd/            # OrchdRpcClient + 协议类型
│       └── session/          # SessionManager（JSONL）
│
├── session/                  # TypeScript — JSONL session 存储（piko-session）
├── cli/                      # TypeScript — CLI 入口
└── sandbox/                  # Rust — 沙箱执行
```
