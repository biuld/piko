# piko Stateless Engine Abstraction

## Goal

目标是抽象出一个**无状态 engine**，包揽：

- 全部 LLM 交互
- 全部工具执行
- tool loop / step budget / stop policy
- 审批暂停与恢复
- sandbox / MCP / 远端执行后端

在这个目标下，`piko` 不再拥有 agent runtime 语义。`piko` 只保留 host 职责：

- session / transcript 持久化
- TUI / approval UI
- 用户输入与 slash commands
- session tree / fork / compact
- engine 事件转 UI

换句话说，`packages/agent` 当前的 loop 不再是系统核心；新的 core 是 stateless engine protocol。

## Non-Goals

这个方案**不考虑**对当前 Pi 扩展语义的兼容：

- 不保留 `beforeToolCall`
- 不保留 `afterToolCall`
- 不保留 `prepareNextTurn`
- 不保留 `shouldStopAfterTurn`
- 不保留 `getSteeringMessages`
- 不保留 `getFollowUpMessages`
- 不保留 `after_provider_response` 现有触发模型

未来如需类似能力，统一作为新 engine protocol 的一部分设计；不复用现有 `packages/agent` hook 模型。

## Core Constraint

既然目标是 stateless engine，就不要停在“Pi 保留半个 loop、engine 只替换 backend”的中间态。

必须满足两点：

1. engine 输入是**完整快照**
2. engine 输出是**完整结果或明确的暂停点**

engine 不依赖 `piko` 进程内状态；`piko` 也不依赖 engine 内部状态来理解执行结果。

## Architecture

```
┌──────────────────────────────────────────────┐
│ piko Host                                    │
│ session / TUI / approvals / tree             │
└──────────────────┬───────────────────────────┘
                   │
                   │ EngineInput / EngineEvent / EngineOutput
                   ▼
┌──────────────────────────────────────────────┐
│ Stateless Engine                             │
│ provider orchestration                       │
│ tool execution                               │
│ sandbox / MCP / approvals                    │
│ stop policy / max steps                      │
└──────────────────────────────────────────────┘
```

## Naming

命名如下：

- 项目名：`piko`
- 仓库名：`piko`
- CLI：`piko`
- Host 包：`piko-host`
- Engine 协议包：`piko-engine-protocol`
- Native Engine 包：`piko-engine-native`
- Remote Engine 包：`piko-engine-remote`

文档中提到的 `Host` 默认指 `piko Host`。

## Working Directory

固定工作目录如下：

- `~/Projects/piko`

### Host Responsibilities

`piko Host` 负责：

- 维护 session transcript
- 选择 engine 实现
- 构造 `EngineInput`
- 消费 `EngineEvent`
- 在审批请求时展示 UI，并把决定回传给 engine
- 把 `EngineOutput` 写回 session

### Engine Responsibilities

engine 负责：

- 把 transcript 转成 provider request
- 发起和消费 provider stream
- 解析 assistant tool calls
- 执行工具
- 处理多轮 tool loop
- 产生最终 assistant / toolResult transcript
- 决定何时 stop / error / await approval / max steps

## Execution Model

`piko` 采用 **step-based stateless engine**。

主协议只有一个入口：

- `executeStep()`

主调度关系如下：

- `piko Host` 调度 step
- engine 执行单个 step
- approval、abort、持久化恢复全部围绕 step 协议设计

如果需要 run-to-completion 行为，只能在 step 协议之上封装 helper，不新增并行主协议。

## Protocol

### Stateless Engine

```typescript
export interface StatelessEngine {
  readonly capabilities: EngineCapabilities;

  executeStep(
    input: EngineInput,
    signal?: AbortSignal,
  ): EventStream<EngineEvent, EngineStepResult>;

  resolveApproval?(
    request: EngineApprovalResolution,
    signal?: AbortSignal,
  ): Promise<EngineStepResult>;

  shutdown?(): Promise<void>;
}
```

`executeStep()` 必须是无状态的：同样的输入快照应得到同样的语义结果。实现可以内部做缓存，但缓存不能成为协议必需条件。

## EngineInput

`EngineInput` 必须足够完整，能让任何 engine 独立完成一步执行。

```typescript
export interface EngineInput {
  runId: string;
  stepId: string;
  transcript: AgentMessage[];
  systemPrompt: string;
  model: Model<any>;
  provider: EngineProviderConfig;
  tools: EngineTool[];
  settings: EngineRunSettings;
  pendingApproval?: PendingApprovalState;
  engineState?: unknown;
}
```

### Provider Config

```typescript
export interface EngineProviderConfig {
  apiKey?: string;
  headers?: Record<string, string>;
  reasoning?: {
    effort?: string;
    summary?: string;
  };
  sessionId?: string;
  baseUrl?: string;
  extra?: Record<string, unknown>;
}
```

### EngineRunSettings

```typescript
export interface EngineRunSettings {
  maxSteps: number;
  parallelTools: boolean;
  allowToolCalls: boolean;
  allowApprovals: boolean;
  toolChoice?: "auto" | "required" | "none";
  stopConditions?: {
    stopOnAssistantMessage?: boolean;
    stopOnToolResult?: boolean;
  };
}
```

### EngineTool

这里的工具不再是当前 `AgentTool`。

```typescript
export interface EngineTool {
  name: string;
  description: string;
  inputSchema: unknown;
  executor: EngineToolExecutorRef;
  executionMode?: "sequential" | "parallel";
  metadata?: Record<string, unknown>;
}
```

```typescript
export interface EngineToolExecutorRef {
  kind: "native" | "remote" | "sandbox" | "mcp";
  target: string;
  extra?: Record<string, unknown>;
}
```

工具目录是 engine 可直接执行的目录，而不是旧 Pi hook 包装后的工具列表。

## Output Model

### EngineEvent

```typescript
export type EngineEvent =
  | { type: "step_start" }
  | { type: "provider_request"; request: unknown }
  | { type: "provider_response"; status?: number; headers?: Record<string, string> }
  | { type: "message_start"; message: AgentMessage }
  | { type: "message_delta"; messageId: string; delta: unknown }
  | { type: "message_end"; message: AgentMessage }
  | { type: "tool_call_start"; id: string; name: string; args: unknown }
  | { type: "tool_call_update"; id: string; partialResult: unknown }
  | { type: "tool_call_end"; id: string; result: unknown; isError: boolean }
  | { type: "approval_requested"; request: PendingApprovalState }
  | { type: "state_snapshot"; engineState: unknown }
  | { type: "step_end" }
  | { type: "error"; message: string };
```

### EngineStepResult

```typescript
export interface EngineStepResult {
  status: "continue" | "awaiting_approval" | "completed" | "aborted" | "error";
  appendedMessages: AgentMessage[];
  usage?: TokenUsage;
  pendingApproval?: PendingApprovalState;
  engineState?: unknown;
  stopReason?: "assistant" | "tool" | "max_steps" | "approval" | "abort" | "error";
}
```

### PendingApprovalState

```typescript
export interface PendingApprovalState {
  requestId: string;
  kind: string;
  details: unknown;
  engineState?: unknown;
}
```

### EngineApprovalResolution

```typescript
export interface EngineApprovalResolution {
  runId: string;
  stepId: string;
  approvalRequestId: string;
  decision: "accept" | "decline" | "acceptForSession";
  transcript: AgentMessage[];
  engineState?: unknown;
}
```

审批恢复不能依赖进程内挂起协程，必须通过明确的 approval request state 恢复。这是 stateless 设计的关键点。

## Step Semantics

一次 `executeStep()` 可以做以下几种事：

1. 发起 provider 请求并输出 assistant message
2. 执行一批工具并返回 tool result messages
3. 发现需要审批，返回 `awaiting_approval`
4. 命中 stop condition / max steps，返回 `completed`

step 语义固定如下：

- 一个 step 最多消费一次 provider response
- 一个 assistant message 后可执行零个或多个工具
- 如果 assistant 产出 tool calls 且工具无需审批，同一 step 内可以直接执行工具
- 如果任一工具需要审批，该 step 在审批点结束，返回 `awaiting_approval`
- harness 记录 `pendingApproval`，待用户决定后调用 `resolveApproval()`

## Transcript Ownership

在这个方案里，transcript 的**持久化归属**仍然是 `piko Host`，但 transcript 的**生成归属**是 engine。

具体来说：

- engine 决定应追加哪些 `assistant` / `toolResult` messages
- harness 不再自己拼 `toolResult` message
- harness 只把 `appendedMessages` 追加到 session

这意味着当前 `packages/agent` 中这类逻辑将被废弃或下沉到 engine：

- `streamAssistantResponse()`
- `executeToolCalls()`
- `prepareToolCall()`
- `executePreparedToolCall()`
- `finalizeExecutedToolCall()`
- `createToolResultMessage()`

## Package Topology

当前目标放在**独立下游仓库**，不在 `pi-mono` 主仓原地重构。

原因：

- 这是下游二次开发和二次打包
- 新 runtime 明确不兼容当前 `packages/agent` 语义
- 希望复用上游稳定组件，但不继承上游 runtime 结构
- 需要独立控制版本 pinning、发布节奏和 breaking changes

仓库策略如下：

- 不会改动的上游包，直接通过 npm 引入
- 需要重写的 runtime / host 组件，在下游仓库内新建
- 只有确实必须改动的上游包，才做 patch 或单包 fork

目标结构如下：

```text
packages/engine-protocol/    协议与共享类型
packages/engine-native/      本地 stateless engine
packages/engine-remote/      JSON-RPC client
packages/host-runtime/       harness / session / TUI / product shell
```

上游依赖如下：

- `@earendil-works/pi-ai`
- `@earendil-works/pi-tui`
- 其他你确认不会修改的 Pi 组件

不整仓 fork `pi-mono`。执行顺序如下：

1. 直接依赖上游 npm 包
2. 下游仓库内自研新 runtime
3. 必要时 patch 上游
4. 最后才考虑单包 fork

## Native Engine

`engine-native` 是默认实现，直接在 Node 进程内完成：

- provider 调用
- 工具执行
- step budget 控制
- approval pause / resume

它复用当前 `packages/agent/src/agent-loop.ts` 的实现思路，但不是简单提取函数，而是重组为新的 state machine。

内部模块拆分如下：

```typescript
packages/engine-native/src/
  engine.ts
  state-machine.ts
  provider-runner.ts
  tool-runner.ts
  approval-state.ts
  transcript-builder.ts
```

## Remote Engine

远端 engine 应该和本地 engine 共享同一协议，而不是暴露 Pi 内部细节。

### JSON-RPC

```typescript
{ method: "engine/execute_step", params: EngineInput, id: 1 }
{ method: "engine/resolve_approval", params: EngineApprovalResolution, id: 2 }
{ method: "engine/shutdown", id: 3 }
```

通知：

```typescript
{ method: "engine/event", params: EngineEventEnvelope }
```

结果：

```typescript
{ id: 1, result: EngineStepResult }
{ id: 2, result: EngineStepResult }
{ id: 3, result: {} }
```

其中：

```typescript
export interface EngineEventEnvelope {
  runId: string;
  stepId: string;
  event: EngineEvent;
}
```

统一成单一 `engine/event` notification，transport 更简单，也更便于多语言实现。

## Host Runtime Flow

`piko Host` 的运行流程如下：

```typescript
Harness.run()
  -> build EngineInput
  -> engine.executeStep()
  -> append EngineStepResult.appendedMessages
  -> if status === "continue" then next step
  -> if status === "awaiting_approval" then show UI and resolveApproval()
  -> if status === "completed" then end run
```

也就是说，`piko Host` 变成 engine scheduler，而不是 agent runtime。

## Engine State

严格无状态的理想形态是不返回 `engineState`。但现实里审批暂停、远端 sandbox handle、MCP session、provider continuation token 都可能需要跨 step 恢复。

协议允许 `engineState`，要求如下：

- Pi 不解释它
- Pi 只存储和回传它
- engine 必须在缺少该状态时优雅失败
- engineState 不得替代 transcript 成为唯一真实来源

这样协议仍然是“逻辑无状态”，只是允许实现做 opaque checkpoint。

## Step Model Constraints

step-based 是唯一执行模型：

- approval 按 step 暂停和恢复
- abort / interrupt 作用于当前 step
- UI 按 step 持久化
- remote engine 按 step 观测和恢复

`runToCompletion()` 只能作为 step 协议之上的封装，不定义为并行模型。

## Migration Strategy

### Phase 1: Protocol

1. 新建 `packages/engine-protocol/`
2. 定义 `EngineInput` / `EngineEvent` / `EngineStepResult`
3. 明确 transcript 与 approval 的协议语义

### Phase 2: Native Engine

1. 新建 `packages/engine-native/`
2. 用当前 agent-loop 逻辑重写为 step state machine
3. 保证 assistant/tool transcript 由 engine 生成

### Phase 3: Host Runtime Integration

1. `packages/host-runtime` 直接驱动 engine step
2. session manager 只负责存储和展示，不再拥有 tool/runtime 逻辑
3. 旧 `packages/agent` 只保留过渡 facade

### Phase 4: Remote Engine

1. 实现 `packages/engine-remote/`
2. 对接 Codex / Rust / sandbox backend
3. 把工具目录改成 executor-ref 模型

### Phase 5: Delete Old Runtime

1. 删除或降级 `packages/agent` 中旧 loop
2. 删除旧 hook 兼容层
3. 统一所有新 runtime 到 engine protocol

## Summary

本项目的拆分结果如下：

- Pi 只做 harness
- engine 做完整 runtime
- 输入是完整快照
- 输出是事件流 + step result
- approval 通过显式状态恢复
- transcript 由 engine 生成，Pi 只负责持久化

`piko` 采用这条路径，并以此作为后续接 Codex、Rust 和远端沙箱的固定边界。
