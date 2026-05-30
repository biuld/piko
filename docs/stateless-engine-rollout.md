# piko Stateless Engine Rollout Plan

## Goal

这份文档是 [engine-abstraction.md](/Users/biu/Projects/pi-mono/docs/engine-abstraction.md) 的落地补充。

目标不是在 `pi-mono` 主仓内一次性重写 Pi，而是在 `piko` 这个**独立下游仓库**里按阶段落地：

- `piko Host` 负责 session / UI / 调度
- Stateless Engine 负责 LLM 交互 / 工具执行 / approval / step state machine

这份计划按“可并行、可回滚、可验证”的原则拆分，便于多个 agent 同时推进。

## Scope

### In Scope

- 新建 stateless engine protocol
- 新建 native engine
- 新建 host runtime，以 step scheduler 方式驱动 engine
- 明确哪些上游包直接依赖，哪些下游自研
- 为后续 remote engine 预留协议

### Out of Scope

- 保持现有扩展 hook 兼容
- 第一阶段接入真实 remote engine
- 第一阶段迁移全部 slash command 逻辑
- 第一阶段 fork 整个 `pi-mono`

## Naming

命名如下：

- 项目名：`piko`
- 仓库名：`piko`
- CLI：`piko`
- Host 包：`piko-host`
- Engine 协议包：`piko-engine-protocol`
- Native Engine 包：`piko-engine-native`
- Remote Engine 包：`piko-engine-remote`

以下命名在本项目中视为**冻结决策**：

- 仓库目录名：`piko`
- 默认 CLI 可执行文件名：`piko`
- Host 包目录：`packages/host-runtime`
- Protocol 包目录：`packages/engine-protocol`
- Native Engine 包目录：`packages/engine-native`
- Remote Engine 包目录：`packages/engine-remote`

## Working Directory

固定工作目录如下：

- `~/Projects/piko`

该目录必须由 git 管理：

- 在 `~/Projects/piko` 初始化 git 仓库
- 所有阶段性产物通过 commit 固化
- 每个 phase 结束前必须存在可回滚提交点

## Repository Strategy

采用**独立下游仓库 + 上游稳定包依赖 + 少量定点 patch/fork**。

### Upstream Dependency Policy

#### Direct npm dependencies

直接依赖这些稳定组件：

- `@earendil-works/pi-ai`
- `@earendil-works/pi-tui`
- 其他确认不会修改的上游包

#### Downstream-owned packages

下游仓库内自研：

- `packages/engine-protocol`
- `packages/engine-native`
- `packages/engine-remote`
- `packages/host-runtime`

#### If an upstream package must change

顺序如下：

1. 上游提 PR
2. 下游 patch
3. 单包 fork

不整仓 fork `pi-mono`。

## Frozen Decisions

以下事项在当前阶段**不再讨论**，实现时直接照此执行：

| 主题 | 决策 |
|---|---|
| 总体形态 | 独立下游仓库，不在 `pi-mono` 主仓开发 |
| Runtime 模型 | step-based stateless engine |
| 主调度者 | `piko Host` |
| 工作目录 | `~/Projects/piko` |
| 版本管理 | `~/Projects/piko` 必须由 git 管理 |
| Transcript 生成 | engine 负责 |
| Transcript 持久化 | host 负责 |
| 审批恢复 | `pendingApproval + engineState` 显式恢复 |
| 上游依赖策略 | 稳定包直接 npm 引入 |
| 不兼容策略 | 不保留旧 `packages/agent` hook 语义 |
| 远端协议 | JSON-RPC，单一 `engine/event` notification |
| 包结构 | `engine-protocol` / `engine-native` / `engine-remote` / `host-runtime` |
| 最小可运行产品 | 先做 terminal host，不先做复杂扩展系统 |

如果实现遇到和这些决策冲突的点，默认先适配本表，而不是回头修改架构。

## Bootstrap Spec

这一节是新仓库初始化脚手架规范。其他 agent 直接据此创建仓库。

### Workspace Layout

```text
~/Projects/piko/
  .git/
  package.json
  package-lock.json
  tsconfig.base.json
  tsconfig.json
  .gitignore
  README.md
  docs/
    engine-abstraction.md
    stateless-engine-rollout.md
  packages/
    engine-protocol/
    engine-native/
    engine-remote/
    host-runtime/
```

### Root `package.json`

根包使用 npm workspaces。

必须包含：

- `private: true`
- `workspaces: ["packages/*"]`
- `type: "module"`

脚本如下：

```json
{
  "scripts": {
    "check": "tsc -b --pretty false",
    "clean": "rm -rf packages/*/dist",
    "build": "tsc -b",
    "test": "npm run check"
  }
}
```

约束：

- 第一阶段不要引入复杂构建器
- 第一阶段全部用 TypeScript project references
- 第一阶段不要引入 bundler-specific 假设

### Root TypeScript Config

根 `tsconfig.base.json` 约束：

- `target: "ES2022"`
- `module: "NodeNext"`
- `moduleResolution: "NodeNext"`
- `strict: true`
- `declaration: true`
- `composite: true`
- `noEmitOnError: true`

根 `tsconfig.json` 只负责 references。

### Git Bootstrap

初始化步骤固定如下：

1. `mkdir -p ~/Projects/piko`
2. `cd ~/Projects/piko`
3. `git init`
4. 创建根目录脚手架
5. 创建首次提交

首次提交 message 固定为：

- `bootstrap workspace`

### Initial Dependency Policy

第一阶段依赖只允许这几类：

- `typescript`
- `@earendil-works/pi-ai`
- `@earendil-works/pi-tui`
- 必需的小型类型库

第一阶段不要引入：

- 状态管理框架
- RPC 框架大而全 SDK
- 额外 UI 框架
- 测试之外的数据库依赖
- 复杂日志平台 SDK

## Package Contracts

这部分定义每个包的唯一职责、允许依赖和禁止项。实现时按这里执行。

### `packages/engine-protocol`

#### Owns

- `StatelessEngine` 接口
- `EngineInput`
- `EngineEvent`
- `EngineStepResult`
- `EngineTool`
- `PendingApprovalState`
- `EngineApprovalResolution`
- `TokenUsage`

#### May Depend On

- `@earendil-works/pi-ai`

#### Must Not Depend On

- `host-runtime`
- `engine-native`
- `engine-remote`
- 旧 `packages/agent` 语义层

#### Must Export

- 单一 public entrypoint：`src/index.ts`
- 所有协议类型从 index re-export

#### Must Not Implement

- provider 调用
- tool 执行
- session 持久化

### `packages/engine-native`

#### Owns

- step state machine
- provider runner
- tool runner
- approval pause/resume
- transcript builder

#### May Depend On

- `piko-engine-protocol`
- `@earendil-works/pi-ai`

#### Must Not Depend On

- `host-runtime`
- `pi-tui`

#### Public Surface

- `createNativeEngine(options?)`
- `NativeEngine` 类型导出

#### Must Not Own

- CLI 参数解析
- TUI 渲染
- session tree

### `packages/engine-remote`

#### Owns

- JSON-RPC transport client
- `engine/event` notification handling
- `executeStep` / `resolveApproval` remote invocation

#### May Depend On

- `piko-engine-protocol`

#### Must Not Depend On

- `host-runtime`
- `pi-tui`
- native engine internals

### `packages/host-runtime`

#### Owns

- engine scheduler
- session store
- approval controller
- model/config loading
- TUI integration
- CLI integration entrypoints

#### May Depend On

- `piko-engine-protocol`
- `piko-engine-native`
- `piko-engine-remote`
- `@earendil-works/pi-tui`
- `@earendil-works/pi-ai`

#### Must Not Own

- provider request translation internals
- tool execution internals
- transcript generation logic

#### Must Not Do

- 手工拼接 `toolResult` message
- 手工执行 tool loop
- 复刻旧 `packages/agent` hook 语义

## Initial Public APIs

下面这些 API 视为第一阶段最小 public surface，其他 agent 不需要再自行设计。

### `engine-protocol`

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

### `engine-native`

```typescript
export interface CreateNativeEngineOptions {
  cwd?: string;
}

export function createNativeEngine(
  options?: CreateNativeEngineOptions,
): StatelessEngine;
```

### `host-runtime`

```typescript
export interface HostRunOptions {
  engine: StatelessEngine;
}

export class PikoHost {
  constructor(options: HostRunOptions);
  run(prompt: string, signal?: AbortSignal): Promise<void>;
}
```

这是第一阶段最小壳层 API。不要在 Phase 1-3 过早扩展公共接口。

## No-Decision Implementation Order

其他 agent 按下面顺序施工，不需要再决定依赖顺序：

1. 建根 workspace
2. 建 `engine-protocol`
3. 建 `engine-native`
4. 给 `engine-native` 补最小测试
5. 建 `host-runtime`
6. 用 `host-runtime` 接 `engine-native`
7. 最后建 `engine-remote` 骨架

并行原则：

- `engine-protocol` 未冻结前，其他包不提交对外 API
- `host-runtime` 在 `engine-native` 的 fake engine 上先联调
- `engine-remote` 不阻塞前 4 步

## Commit Policy

commit 规则如下：

- 每个 phase 至少一个独立 commit
- 每个 task 完成后至少一个独立 commit
- 不把多个 phase 的主成果混到同一个 commit
- commit message 只描述结果，不描述过程

本项目使用以下 commit messages：

- `bootstrap workspace`
- `freeze stateless engine spec`
- `add engine protocol package`
- `add native engine state machine`
- `add host runtime scheduler`
- `integrate upstream pi-ai and pi-tui`
- `add remote engine skeleton`

## Task Board

下面是直接分配给其他 agent 的任务单。

### Task A: Workspace Bootstrap

#### Scope

- 根目录脚手架
- npm workspaces
- TypeScript project references

#### Deliverables

- 根 `package.json`
- 根 `tsconfig.base.json`
- 根 `tsconfig.json`
- 4 个包目录骨架
- 根 `.gitignore`
- git 仓库初始化完成

#### Acceptance

- `npm install` 成功
- `npm run check` 可跑通空包

#### Commit

- `bootstrap workspace`

### Task B: Protocol Package

#### Scope

- `packages/engine-protocol`

#### Deliverables

- 全部冻结协议类型
- `index.ts` re-export
- README 简述

#### Acceptance

- 其他包可仅通过 public exports 编译
- 没有循环依赖

#### Commit

- `add engine protocol package`

### Task C: Native Engine Core

#### Scope

- `packages/engine-native`

#### Deliverables

- `createNativeEngine()`
- `executeStep()`
- `resolveApproval()`
- 最小 transcript builder

#### Acceptance

- assistant-only path 正常
- tool path 正常
- approval path 正常

#### Commit

- `add native engine state machine`

### Task D: Host Scheduler

#### Scope

- `packages/host-runtime`

#### Deliverables

- `PikoHost`
- engine loop scheduler
- session-store 最小实现

#### Acceptance

- 能用 native engine 跑一个 prompt
- 能展示 assistant 和 tool result

#### Commit

- `add host runtime scheduler`

### Task E: Remote Engine Skeleton

#### Scope

- `packages/engine-remote`

#### Deliverables

- transport interface
- remote engine client skeleton
- fake transport tests

#### Acceptance

- 类型可编译
- fake transport 下 `executeStep` 能跑通

#### Commit

- `add remote engine skeleton`

## Detailed Phase Breakdown

| Phase | 目标 | 可交付物 |
|---|---|---|
| 0 | 冻结协议和边界 | 协议文档、职责矩阵、迁移计划 |
| 1 | 引入 engine protocol 包 | `packages/engine-protocol` |
| 2 | 实现 native engine state machine | `packages/engine-native` |
| 3 | Host 接入 step scheduler | `packages/host-runtime` 跑 engine |
| 4 | 接入上游稳定组件并替换壳层 | host 与上游包接通 |
| 5 | 预留 remote engine 接口 | `packages/engine-remote` 骨架 |

## Phase 0: Freeze The Contract

### Objective

先冻结协议，避免实现过程中反复改边界。

### Deliverables

- `docs/engine-abstraction.md`
- 本文档
- 最小类型草案：
  - `EngineInput`
  - `EngineEvent`
  - `EngineStepResult`
  - `PendingApprovalState`
  - `EngineApprovalResolution`
  - `EngineTool`

### Commit

- `freeze stateless engine spec`

### Exit Criteria

- Host / Engine 边界达成一致
- 明确哪些现有 `packages/agent` 语义不迁移
- 明确哪些上游包直接依赖，哪些下游自研
- 后续 agent 可以直接开始建包和写代码

- 1 个 agent，负责文档和协议口径

## Phase 1: Create `packages/engine-protocol`

### Objective

把 engine 协议从具体实现里抽出来，形成稳定的共享类型包。

### Work Items

1. 新建 `packages/engine-protocol`
2. 导出最小协议类型
3. 定义 step 状态和 approval 状态
4. 定义 engine event 类型
5. 定义 tool executor reference 类型
6. 加最小 README 或 package docs

### File Layout

```text
packages/engine-protocol/
  src/
    index.ts
    engine.ts
    events.ts
    tool.ts
    approval.ts
    usage.ts
  package.json
  tsconfig.json
  README.md
```

### Design Rules

- 只放协议，不放实现
- 不依赖 `packages/host-runtime`
- 尽量减少对旧 `packages/agent` 类型的耦合
- 如需复用 message 类型，从 `@earendil-works/pi-ai` 引

### Verification

- `npm run check`

### Parallelization

可与 Phase 2 的 state machine 设计并行，但最终接口以本阶段产物为准。

### Commit

- `add engine protocol package`

## Phase 2: Build `packages/engine-native`

### Objective

实现第一个可运行的 stateless engine，本地跑 provider 和工具。

### Work Items

1. 新建 `packages/engine-native`
2. 实现 `executeStep()`
3. 实现 `resolveApproval()`
4. 实现 provider runner
5. 实现 tool runner
6. 实现 transcript builder
7. 实现 approval pause/resume
8. 实现 max steps / stop policy

### Internal Modules

```text
packages/engine-native/src/
  index.ts
  engine.ts
  state-machine.ts
  provider-runner.ts
  tool-runner.ts
  approval-state.ts
  transcript-builder.ts
  types.ts
```

### Implementation Guidance

#### State Machine

显式建状态机，不把旧 `runLoop()` 直接粘过来。

状态如下：

- `idle`
- `requesting_assistant`
- `executing_tools`
- `awaiting_approval`
- `completed`
- `error`

#### Transcript

engine 必须自己生成：

- assistant message
- tool result message

Host 只做持久化，不再自己拼消息。

#### Approval

遇到 approval 时：

1. 发 `approval_requested`
2. 返回 `EngineStepResult { status: "awaiting_approval" }`
3. 把恢复所需信息写进 `pendingApproval` / `engineState`

#### Engine State

`engineState` 必须是 opaque blob：

- Host 只存储
- Host 不解释
- engine 用它恢复 step

### Verification

- `npm run check`
- 新增最小单测，至少覆盖：
  - assistant-only step
  - assistant -> tool -> tool result
  - approval pause / resume
  - tool error
  - max steps stop

### Assignment

- Agent A: provider runner
- Agent B: tool runner + approval
- Agent C: transcript builder + tests

### Commit

- `add native engine state machine`

## Phase 3: Host Scheduler Integration

### Objective

实现下游 `packages/host-runtime`，直接作为 engine step scheduler。

### Work Items

1. 新建 `packages/host-runtime`
2. 引入 engine 实例配置
3. 构建 `EngineInput`
4. 调 `executeStep()`
5. 消费 `EngineEvent`
6. 持久化 `EngineStepResult.appendedMessages`
7. 处理 `awaiting_approval`
8. approval 后调用 `resolveApproval()`
9. run 完成后更新 session state

### Required Flow

把当前“Agent 驱动器”改造成“Host + Engine 调度器”：

```typescript
while (true) {
  const input = buildEngineInput(sessionState);
  const result = await engine.executeStep(input);
  persist(result.appendedMessages);

  if (result.status === "awaiting_approval") {
    const decision = await promptForApproval(result.pendingApproval);
    const resumed = await engine.resolveApproval(...);
    persist(resumed.appendedMessages);
    continue;
  }

  if (result.status === "continue") continue;
  break;
}
```

### File Layout

```text
packages/host-runtime/
  src/
    index.ts
    host.ts
    scheduler.ts
    session-store.ts
    approval-controller.ts
    model-config.ts
```

### Risks

- event 顺序和 UI 渲染耦合会重新设计
- session 数据结构可能隐含旧 runtime 假设
- approval 流程和用户输入并发需要重新定义

### Verification

- `npm run check`
- 跑与 session / approval 相关的最小测试

### Assignment

- Agent A: scheduler 和 engine wiring
- Agent B: session persistence
- Agent C: approval UI flow

### Commit

- `add host runtime scheduler`

## Phase 4: Integrate Upstream Stable Components

### Objective

把下游 host/runtime 接到上游稳定组件上，形成可运行产品壳。

### Work Items

1. 接入 `@earendil-works/pi-ai`
2. 接入 `@earendil-works/pi-tui`
3. 接入配置加载和模型选择
4. 接入 slash command 壳层
5. 接入 transcript/session 持久化
6. 对必须修改的上游点，确定 patch 还是单包 fork

### Outputs

- 下游仓库可以基于上游稳定包跑起来
- host/runtime 与上游依赖边界清楚
- 必须改动的上游点形成显式清单

### Verification

- `npm run check`
- 跑与 session / approval 相关的最小测试

### Assignment

- Agent A: scheduler 和 engine wiring
- Agent B: session persistence
- Agent C: approval UI flow

### Commit

- `integrate upstream pi-ai and pi-tui`

## Phase 5: Remote Engine Scaffold

### Objective

在不接真实远端实现的前提下，先把 transport 和 client 形状搭好。

### Work Items

1. 新建 `packages/engine-remote`
2. 定义 JSON-RPC client
3. 映射 `engine/event` notification
4. 映射 `execute_step` / `resolve_approval`
5. 增加 fake transport 测试

### File Layout

```text
packages/engine-remote/
  src/
    index.ts
    remote-engine.ts
    transport.ts
    protocol.ts
  test/
```

### Verification

- `npm run check`
- 针对 fake transport 的单测

### Commit

- `add remote engine skeleton`

## Workstreams

多 agent 并行按以下 workstream 拆分：

### Workstream A: Protocol

- Phase 0
- Phase 1
- 维护跨包类型口径

### Workstream B: Native Runtime

- Phase 2
- state machine
- provider/tool/approval internals

### Workstream C: Host Integration

- Phase 3
- `host-runtime` scheduler 化
- session 和 UI 适配

### Workstream D: Upstream Integration

- Phase 4
- 接上游 npm 依赖
- 做 patch / fork 清单

### Workstream E: Remote Prep

- Phase 5
- remote client
- transport 测试骨架

## Dependency Graph

```text
Phase 0 -> Phase 1
Phase 1 -> Phase 2
Phase 1 -> Phase 3
Phase 2 -> Phase 3
Phase 3 -> Phase 4
Phase 1 -> Phase 5
```

其中：

- Phase 2 和 Phase 3 可以部分并行
- 但 Host 真正切换到 engine 之前，native engine 至少要有最小可运行路径

## Migration Checkpoints

### Checkpoint 1

`packages/engine-protocol` 完成，类型稳定。

对应 commit：

- `add engine protocol package`

### Checkpoint 2

`packages/engine-native` 可以离线跑通：

- assistant-only
- assistant + tool
- approval

对应 commit：

- `add native engine state machine`

### Checkpoint 3

`packages/host-runtime` 可以通过 engine 跑主路径。

对应 commit：

- `add host runtime scheduler`

### Checkpoint 4

下游主路径不再依赖旧 `packages/agent` loop。

对应 commit：

- `integrate upstream pi-ai and pi-tui`

### Checkpoint 5

remote engine client 骨架完成。

对应 commit：

- `add remote engine skeleton`

## Risks

### Risk 1: 旧 `packages/agent` 类型污染新协议

处理：

- 新协议包依赖 `pi-ai` 基础 message 类型
- 不把旧 hook 类型带进 engine protocol

### Risk 2: approval 恢复状态设计不清

处理：

- Phase 1 就冻结 `PendingApprovalState`
- 先允许 opaque `engineState`

### Risk 3: Host 与 Engine 双方都在拼 transcript

处理：

- 明确 transcript 生成权只在 engine
- Host 只能追加保存

### Risk 4: 下游仓库不自觉复制上游 runtime 包袱

处理：

- 直接在独立仓库内以新 protocol 为中心建包
- 不复制 `packages/agent` 结构当作起点

## Definition Of Done

满足以下条件可认为迁移主目标完成：

- 下游 host 主路径由 stateless engine 驱动
- engine 能处理 LLM + tool loop + approvals
- transcript 由 engine 生成
- Host 只负责 UI / session / scheduling
- 不依赖旧 `packages/agent` loop 作为主路径核心

## Task Template For Other Agents

给其他 agent 分工时，每个任务都包含：

1. 目标包和文件范围
2. 不可跨越的边界
3. 最小验收标准
4. 需要补的测试

模板如下：

```text
Task:
- Implement <feature> in <package>

Constraints:
- Do not reintroduce old agent-loop semantics
- Keep transcript generation inside engine
- Do not add compatibility hooks unless explicitly requested

Verification:
- Run npm run check
- Run the focused tests you add or modify
```
