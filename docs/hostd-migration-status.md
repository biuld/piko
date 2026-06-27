# Hostd 迁移状态梳理

## 1. 目标与背景

当前项目的核心任务是**将原有的 TypeScript 版本的 `host-runtime` 迁移并统一到一个纯 Rust 实现的 Server（即 `hostd` + `orchd`）中**。

这次迁移的核心驱动力是**基于统一的事件模型（Event Model）**。通过全新设计的 `HostEvent`，彻底重构了之前的状态和通信机制，作为整个架构的核心能力。

迁移完成后的最终架构将会是：`TUI -> Server (hostd + orchd)`。

---

## 2. 核心架构：Event 模型的设计与落实

参考 `docs/design/host-event-design.md`，核心理念是以 **Session -> Turn -> Task -> Step -> Message** 为执行层级，并通过 **EventBus** 串联状态重放与流式推送。

### 2.1 事件统一化
原来 TS 版本中存在多层复杂且冗余的事件抽象（如 `OrchestratorEvent`, `RuntimeEvent`, `OrchEvent`, `OrchSourcingEvent`, `pi-compat HostEvent` 等，共计 64 种变体）。
现在已经在 Rust 端（`packages/orchd/src/protocol/host_event.rs`）统一收敛为 **21 种 `HostEvent`**，严格分为两类：
- **Domain Events (15 种)**：持久化写入 Session JSONL 日志中，用于完整无损的状态重建（State Rebuild），如 `UserMessageSubmitted`, `TaskCompleted`, `TurnStarted` 等。
- **Streaming Events (6 种)**：只作为实时推送到客户端（TUI）的流式事件，不落盘，如 `TextDelta`, `ThinkingDelta`, `ToolStart`, `ApprovalRequested` 等。

### 2.2 Rust 端落地状态
`HostEvent` 枚举定义已经完整落盘并在 `hostd` 与 `orchd` 中通信使用，替代了原来的 JSON-RPC。
通过在 JSONL 格式下追加 Domain 事件，奠定了可靠的状态存储基础（完全兼容 pi v3 格式）。

---

## 3. 当前迁移现状 (Gap Analysis)

目前 `hostd` (约 1500 行 Rust) 已经完成了基础框架和核心运转流程，但相较于庞大复杂的 `host-runtime` (约 10000+ 行 TS)，仍有一些关键的功能 Gap 需要补全。

### ✅ 已完成的迁移功能 (Finished)
1. **核心通信流**：TUI <-> `hostd` <-> `orchd` 的直连管道和基础通信协议（基于 JSON-lines）。
2. **基础会话生命周期**：Session 的 Create、Delete 以及基本的本地 JSONL 追加存储。
3. **Turn/Task 基础执行**：
   - 提交 Prompt 并启动单 Agent 任务流。
   - `EventBus` 的流式事件分发（文本输出、Tool 执行、Approval 生命周期）。
4. **上下文注入**：System prompt 构建，基础的 Context files 挂载，Prompts/Templates 与 Skills 文件的加载。
5. **模型与鉴权基础**：Model Registry 体系，基础 API key 的 `AuthStorage` 管理。

### ❌ 核心功能缺失与下一步计划 (Gaps to fill)

以 `HostEvent` 生命周期和使用场景为线索，当前的缺失主要集中在以下几个领域：

#### 3.1 会话操作的命令分发
- **现状**：`hostd` 的 `JsonlSessionRepository` 已经具备了底层原语（如 fork, navigate, import），但在 `HostServer::apply_command()` 的路由层还没接通。
- **任务**：实现 List, Rename, Fork, Navigate, Import, Switch 会话的命令处理，并抛出相应的 Domain 事件。补充写入 `ModelChange`, `ThinkingLevel` 等 Metadata Domain 事件。

#### 3.2 队列管理 (Queue Management)
- **现状**：完全缺失。现在的 `hostd` 把每次 prompt 都当作一个全新的 Turn。
- **任务**：实现 `HostQueueController`，支持 Steer（干预）、Follow-up（追问）以及 Next-turn 的队列模型。加入 `QueueUpdate` 和 `TaskSteered` 等事件的流转逻辑。

#### 3.3 多 Agent 与监督模型 (Multi-agent & Steering)
- **现状**：目前是单 Agent 单 Task 跑到底，没法 Spawn。
- **任务**：
   - 实现工具级的 `spawn` / `spawn_detached`。
   - 支持跨 Agent/Task 的 Steering，完善并发多任务下的 `TaskCreated`, `TaskTranscriptCommitted` 的上下文合并逻辑。

#### 3.4 MCP 与扩展能力 (MCP & Tooling)
- **现状**：`host-runtime` 里集成了完整的 `McpServerManager`，而在 `hostd` 里还是空白。
- **任务**：用 Rust 重写 MCP Client，支持 IDE/Editor 的集成以及扩展工具的自动发现。

#### 3.5 上下文压缩 (Compaction)
- **现状**：只保留了基础的计算 token 接口，没有实际裁剪执行。
- **任务**：补充实现触发条件逻辑、LLM Summary 生成，并将其持久化为 `CompactionEntry` 到 JSONL 中。

#### 3.6 鉴权与杂项
- **任务**：补全 OAuth 流程（Device flow/PKCE）。完善运行时无重启切模型（Model hot-switch）及相关的 `ModelConfigChanged` 事件处理。

---

## 4. 总结

`hostd` 的迁移已经完成了最艰难的**架构范式转移（Paradigm Shift）**——即将过去的黑盒/强状态 runtime 转变成了**基于 Event-sourcing 模式、统一 `HostEvent` 驱动**的纯粹架构。接下来进入实质性的“特性补齐”阶段，核心工作将围绕着补全 Session 命令分发、队列控制调度、多 Agent 协作以及 MCP 工具链的 Rust 实现展开。
