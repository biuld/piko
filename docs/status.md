# piko 项目现状

## 1. 架构

```
TUI (TypeScript, host-tui)  ←→  hostd (Rust, hostd)  ←→  orchd (Rust, orchd)
       │                              │                        │
       └── JSON-lines ────────────────┘                        │
       (HostEvent, snake_case, 29 variants)                    │
                                                               │
                                              hostd ──OrchCore (direct link)──→ orchd
                                                               (HostEvent direct stream)
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

单一事件类型，29 个变体，snake_case。定义位置：

| 位置 | 语言 |
|------|------|
| `orchd/src/protocol/host_event.rs` | Rust（权威定义） |
| `hostd/src/api/mod.rs` | Rust（re-export + HostCommand/CommandAck） |
| `host-tui/src/client/hostd-protocol.ts` | TypeScript（镜像） |

```
Domain events (21):            Streaming events (8):
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
  session_opened
  session_listed
  state_snapshot
  queue_update
  model_config_changed
```

### HostEvent（Rust 侧唯一对外事件）

当前 Rust 侧 `HostEvent` 已只有一个权威定义：
`orchd::protocol::host_event::HostEvent`，并由 `hostd::api` re-export。
`hostd` 通过 `OrchCore::subscribe_host_events()` 订阅 HostEvent。Rust runtime path 中的
`OrchEvent` / `subscribe_orch()` / HostEvent bridge 已删除。

orchd 的 model streaming、tool streaming 和带 host context 的 task lifecycle
已经直接产出 HostEvent：
`message_start/message_end/text_delta/thinking_delta/tool_start/tool_end/approval_requested`
以及 `task_created/task_started/task_completed/task_failed/task_cancelled`。
完成态 domain commit 也已由 agent loop 直接产出：
`assistant_message_completed/tool_result_committed/task_transcript_committed/task_joined`。
TUI hostd event adapter 已把 root `task_transcript_committed` 接入 reducer，用 canonical transcript
修正 streaming 丢失或乱序后的主 timeline；child task transcript 暂不混入主 transcript。

剩余缺口不是再引入中间事件，而是补齐统一 HostEvent 的 host-visible contract：
task steer、plan state、approval resolved。
`OrchSourcingEvent` 仍是 orchd 内部状态重放日志，不能作为 host 通信事件使用。

```
TaskSteered, ApprovalResolved, plan-state event（待设计）
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

**当前结果**：Rust runtime path 已收敛到 1 层对外 HostEvent；TS TUI mirror 仍需手工同步。
**目标结果**：按 `docs/design/host-event-design.md` 补齐 HostEvent contract，并移除 TS legacy host-runtime 事件层。

---

## 3. 构建状态

| 包 | 语言 | 构建 | 测试 |
|---|------|------|------|
| `orchd` | Rust | ✅ | ✅ 0 fail |
| `hostd` | Rust | ✅ | ✅ 0 fail |
| `host-runtime` | TS | ✅ | ✅ 由 `bun run test` 覆盖 |
| `host-tui` | TS | ✅ | ✅ 由 `bun run test` 覆盖 |

---

## 4. 功能对照：hostd vs host-runtime

### ✅ 已完成

| 功能 | hostd | host-runtime |
|------|-------|-------------|
| 基本 session 创建/打开/删除 | ✅ | ✅ |
| Session list/fork/navigate/import/rename/snapshot/resume | ✅ | ✅ |
| Turn 提交 + 取消 | ✅ | ✅ |
| 流式事件（text/thinking/tool/approval）| ✅ | ✅ |
| System prompt 构建 | ✅ | ✅ |
| Skills 加载 + prompt template | ✅ | ✅ |
| Model registry + API key auth | ✅ | ✅ |
| JSONL 持久化（存储层）| ✅ | ✅ |
| 审批 flow（requested → resolved 闭环）| ✅ | ✅ |
| 模型配置变更通知 + 持久化 | ✅ | — |
| 运行时工具切换（active_tools config）| ✅ | ✅ |
| Task steering（steer_task tool + steering queue）| ✅ | ✅ |
| 队列管理（steer/follow-up/next-turn + auto-drain）| ✅ | ✅ |
| Compaction 执行（LLM 摘要 + 自动触发）| ✅ | ✅ |
| Multi-agent turn（spawn/detach sub-task，track all tasks）| ✅ | ✅ |
| MCP 工具集成（stdio JSON-RPC）| ✅ | ✅ |
| Session 元数据条目（model/thinking/compaction 日志）| ✅ | — |

### ❌ hostd 缺失

| 功能 | 影响 | 复杂度 |
|------|------|--------|
| **OAuth**（device code flow） | 无 OAuth 流程实现 | 中 |
| **Session clone/switch 高层语义** | 底层 list/fork/navigate/import/rename/snapshot/resume 已接线；clone/switch 还需确定 hostd 命令边界 | 低 |

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

### 当前架构债

| 问题 | 当前状态 | 下一步 |
|------|----------|--------|
| 事件层级 | model/tool streaming、host-context task lifecycle、assistant/tool result commit、task transcript commit、task join 已直接产 HostEvent；Rust `OrchEvent`/`subscribe_orch()` 已删除；TUI root transcript commit reducer 已接入 | 补齐 task steer、plan state、approval resolved 的 HostEvent contract；child task transcript 需要独立 UI state |
| hostd → orchd 边界 | `hostd/src/turn_runner.rs` 和 `server.rs` 仍直接引用 `orchd::protocol`/`OrchCore` | 边界修正必须服从统一 HostEvent 设计，不能通过新增 runtime event enum 隔离 |
| JSON-lines ack | `run_jsonl_server()` 已输出 `CommandAck`，避免 TUI command timeout | 后续把执行期错误区分为 command rejection 与 domain failure |

---

## 6. 目录结构

```
packages/
├── hostd/                    # Rust — 宿主守护进程
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── api/mod.rs        # HostCommand/CommandAck + HostEvent re-export
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
│       ├── protocol/         # HostEvent 权威定义 + agents、tools
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
