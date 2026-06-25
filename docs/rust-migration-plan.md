# Rust 迁移方案：Orchestrator 篇

## 概述

将 `packages/orch-protocol` + `packages/orchestrator` 从 TypeScript 迁移至 Rust 一一   
Rust 部件编译为独立二进制，通过 JSON-RPC (sidecar) 与 TS host-runtime 通信，   
未来可演进为纯 Rust CLI 绕过 TS 层。

---

## 架构

```
                    ┌──────────────────────┐
                    │   host-tui (TS)       │  不变
                    └──────┬───────────────┘
                           │
                    ┌──────┴───────────────┐
                    │   host-runtime (TS)   │  替换 Orchestrator 引用
                    │   ┌─────────────┐     │  从 new Orchestrator(engine)
                    │   │OrchClient   │     │  改为 OrchRpcClient
                    │   └──────┬──────┘     │
                    └──────────┼────────────┘
                               │ JSON-RPC over stdio / unix socket
                    ┌──────────┴────────────┐
                    │  orchd (Rust binary)   │  新
                    │  ┌──────────────────┐  │
                    │  │ Orchestrator     │  │
                    │  │ ActorSystem      │  │
                    │  │ ToolRegistry     │  │
                    │  │ ModelExecutor    │  │
                    │  └──────────────────┘  │
                    └────────────────────────┘
```

通信协议：
- 序列化：JSON（与现有 `orch-protocol` 类型兼容）
- 传输：stdio（进程 spawn）或 Unix socket
- 规范：方法调用 → JSON response，event stream → JSON lines push

---

## Rust Workspace 结构

```
packages/orchd/
├── Cargo.toml
├── src/
│   ├── main.rs              # bin: orchd JSON-RPC 入口
│   ├── lib.rs                # lib: 所有 traits + re-export
│   ├── protocol/             # <= piko-orch-protocol
│   │   ├── mod.rs
│   │   ├── agents.rs         # AgentSpec, AgentTask, AgentTaskId
│   │   ├── approval.rs       # ApprovalGateway, ToolApprovalDecision
│   │   ├── commands.rs       # 命令封装 （Host → Orch）
│   │   ├── events.rs         # HostEvent, HostEventListener
│   │   ├── messages.rs       # Message, ToolCall, ToolDef, RuntimeMessage
│   │   ├── model.rs          # ModelProviderConfig, ModelRunSettings
│   │   ├── runtime.rs        # OrchModelConfig, Orchestrator trait, OrchRunResult
│   │   ├── state.rs          # OrchState
│   │   └── tools.rs          # ToolProvider, ToolSet, ToolExecResult
│   ├── kernel/               # <= actor-system + mailbox + envelope
│   │   ├── mod.rs
│   │   ├── actor_system.rs   # ActorSystem
│   │   ├── envelope.rs       # Envelope, ActorId
│   │   └── mailbox.rs        # Mailbox
│   ├── actors/
│   │   ├── mod.rs
│   │   ├── agent.rs          # AgentActor (handler + runner)
│   │   │   ├── mod.rs
│   │   │   ├── handler.rs    # actor message dispatch
│   │   │   ├── runner.rs     # startAgentRun
│   │   │   ├── engine_loop.rs # runEngineLoop
│   │   │   ├── step_runner.rs # runModelStep + processStepOutcome
│   │   │   ├── tool_executor.rs # executeToolCalls (parallel/sequential)
│   │   │   └── types.rs      # AgentActorDeps, WorkerState
│   │   └── state.rs          # EventStore, projections, reducer
│   ├── model/
│   │   ├── mod.rs
│   │   ├── executor.rs       # ModelStepExecutor trait + self-llm 适配
│   │   └── stream.rs         # EventStream adapter
│   ├── tools/
│   │   ├── mod.rs
│   │   ├── registry.rs       # ToolRegistryImpl
│   │   └── orch_provider.rs  # OrchToolProvider (delegate, join, state, plan)
│   ├── orchestrator/
│   │   ├── mod.rs
│   │   ├── facade.rs         # OrchestratorFacade impl
│   │   ├── agent.rs          # registerAgent / unregisterAgent
│   │   ├── task.rs           # dispatch, run, cancelTask, joinTask
│   │   ├── tool.rs           # registerToolSet / Provider / setModelConfig
│   │   ├── state.rs          # snapshot, subscribe, getGraph
│   │   └── context.rs        # OrchestratorContext
│   └── rpc/                  # JSON-RPC server
│       ├── mod.rs
│       ├── server.rs         # axum / stdio transport
│       └── handlers.rs       # 方法 → Orchestrator 调用映射
```

---

## Rust 2024 关键变化

| 变化 | 影响 |
|---|---|
| `async fn` in traits **稳定** | 不再需要 `#[async_trait]` 宏！所有 trait 方法直接 `async fn` |
| `impl Trait` 捕获所有生命周期 | RPIT 不透明类型默认捕获所有泛型参数，需要 `use<..>` 精确捕获 |
| `unsafe_op_in_unsafe_fn` 默认 deny | unsafe 函数内 unsafe 操作需要显式 `unsafe {}` 块 |
| 新 `if let` 临时变量作用域 | 更短的临时变量生命周期 |
| `gen {}` blocks | 仍 unstable，暂不用 |

## 依赖清单

```toml
[package]
name = "orchd"
version = "0.1.0"
edition = "2024"

[dependencies]
tokio = { version = "1", features = ["full"] }
tokio-actors = "0.6"           # Actor 框架
self-llm = "0.1"               # OpenAI + Anthropic LLM
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"                # 结构化日志
tracing-subscriber = "0.3"
anyhow = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4"] }
chrono = "0.4"
futures-core = "0.3"           # Stream trait
pin-project-lite = "0.2"

# 2024 edition: async fn in trait 已稳定，不需要 #[async_trait]
```

---

## 关键映射：TS → Rust

### ActorSystem

| TS | Rust (tokio-actors) |
|---|---|
| `ActorSystem` class | `ActorSystem` struct |
| `spawn(spec) → ActorRef` | `MyActor::default().spawn().named("x").await?` |
| `send(to, msg)` | `handle.notify(msg)?` |
| `ask<T>(to, msg) → Promise<T>` | `handle.send(msg).await?` |
| `stop(id)` | `handle.stop(StopReason::Graceful).await?` |
| `ActorContext` | `ActorContext<MyActor>` |

### Orchestrator trait

| TS (`interface Orchestrator`) | Rust (`#[async_trait] trait Orchestrator`) |
|---|---|
| `registerAgent(spec)` | `async fn register_agent(&self, spec: AgentSpec) -> Result<()>` |
| `dispatch(task) → Promise<AgentTaskId>` | `async fn dispatch(&self, task: AgentTask) -> Result<AgentTaskId>` |
| `run(prompt, opts) → Promise<OrchRunResult>` | `async fn run(&self, prompt: &str, opts: OrchRunOptions) -> Result<OrchRunResult>` |
| `subscribe(listener) → () => void` | 通过 JSON-RPC push events，不映射同步 callback |

### ModelStepExecutor

| TS (`executeStep(input, signal) → EventStream`) | Rust |
|---|---|
| `EventStream<ModelStepEvent, ModelStepResult>` | `impl Stream<Item = ModelStepEvent>` + `oneshot` 返回 final result |
| 流内事件：`message_delta`, `thinking_delta`, `message_start`... | 不变，serde 序列化 |
| `self-llm::Client.chat_stream()` | 适配为 `Stream<ModelStepEvent>` |

---

## 执行阶段

### Phase 1：Rust 骨架 + protocol 类型 (2-3 天)

**目标**：`cargo build` 编译通过，protocol 类型 100% 覆盖 TS  

- [ ] `cargo init packages/orchd`
- [ ] 复制 `orch-protocol` 所有类型为 Rust struct/enum/trait
- [ ] 所有类型 `#[derive(Serialize, Deserialize)]`
- [ ] `Orchestrator` trait 定义（async trait）
- [ ] CI：`cargo build && cargo test`

### Phase 2：Kernel — ActorSystem (1-2 天)

**目标**：`tokio-actors` 替换手写 ActorSystem  

- [ ] 用 `tokio-actors::Actor` trait 实现基本 actor
- [ ] 验证 `spawn`, `notify`, `send` (ask), `stop`
- [ ] 集成测试：ping-pong actor

### Phase 3：Model Executor (1-2 天)

**目标**：self-llm 适配 `ModelStepExecutor` trait  

- [ ] `self-llm::Client` 包装为 `OpenAiExecutor` / `AnthropicExecutor`
- [ ] SSE stream 转换为 `Stream<Item = ModelStepEvent>`
- [ ] 集成测试：真实 API 调用 + mock

### Phase 4：AgentActor (3-4 天)

**目标**：engine loop 完整翻译  

- [ ] AgentActor 作为 `tokio_actors::Actor`
- [ ] `runEngineLoop` (while true: → discover → callModel → processOutcome → repeat)
- [ ] `runModelStep` + `processStepOutcome`
- [ ] Tool execution: parallel / sequential
- [ ] AbortSignal → tokio `CancellationToken`

### Phase 5：ToolRegistry (1-2 天)

**目标**：工具发现 + 审批 + 执行  

- [ ] `ToolRegistryImpl` + `OrchToolProvider`
- [ ] Approval gateway（可选，先 stub）

### Phase 6：Orchestrator Facade (1-2 天)

**目标**：`Orchestrator` trait 的完整实现  

- [ ] `OrchestratorFacade` 实现 `Orchestrator` trait
- [ ] `dispatch`, `run`, `cancel`, `subscribe` → EventStore
- [ ] `snapshot`, `getGraph`

### Phase 7：JSON-RPC Server + TS 适配 (2-3 天)

**目标**：TS host-runtime 能通过 JSON-RPC 调用 Rust orchestrator  

- [ ] `OrchRpcServer`: JSON-RPC 2.0 over stdio
- [ ] 所有 `Orchestrator` trait 方法暴露为 RPC 调用
- [ ] Event stream → JSON lines push
- [ ] TS 侧 `OrchRpcClient` 实现 `Orchestrator` interface
- [ ] 集成测试：TS → RPC → Rust → 返回

### Phase 8：测试 + CI (1-2 天)

**目标**：parity with TS tests  

- [ ] 翻译 TS 测试到 Rust (`#[tokio::test]`)
- [ ] Mock provider 用于 integration test
- [ ] CI: `cargo test`, `cargo clippy`

---

## 不在范围内

- TS `host-runtime` 的其他模块（session, auth, settings, compaction）
- `host-tui` 重写
- MCP provider
- 分布式 actor (cluster)

---

## 决策记录

| 日期 | 决策 | 依据 |
|---|---|---|
| 2026-06-23 | 选用 `self-llm` 做 LLM adapter | OpenAI + Anthropic 覆盖，与 pi-ai 功能对等 |
| 2026-06-23 | 选用 `tokio-actors` 做 Actor 框架 | 1:1 映射当前 ActorSystem，有 supervision tree |
| 2026-06-23 | sidecar 模式（JSON-RPC over stdio） | 不绑定 Node ABI，独立分发，未来可纯 Rust CLI |
| 2026-06-23 | 先不定义 `host-protocol` | 重点在 orchestrator 侧，host 边界后续重构 |

---

## TODO 追踪

见 [todo list](#)，由 pi-todo 工具跟踪。每个 Phase 由一个 subagent 负责执行。

---

**状态**：Draft → 待开始 Phase 1
