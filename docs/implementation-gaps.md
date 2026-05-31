# piko — Implementation Gaps & Action Plan

> 基于 2026-05-31 当前代码复核。
>
> 目标口径：**不追求完整复刻 pi 的 extension runtime**；只关注 `host-runtime` / `host-tui` 对 `pi-mono` 的 `coding-agent + agent core` 主线能力复刻。

---

## 总览

```text
piko 现在:  host + engine 架构 ✓ · 基础会话/TUI/工具链贯通 ✓ · 多数表层功能已接上 ✓
pi 目标:    coding agent + agent core 行为语义等价
当前差距:  agent lifecycle / turn state / queue semantics / session write semantics 仍未等价
```

历史文档里的若干 P0/P1 已经完成：

- SettingsManager 接入 CLI/TUI/Host
- ModelRegistry + AuthStorage 基础接线
- runtime model switching 通过 `PikoHost.setConfig()` 影响下一轮请求
- scoped model cycling 与 `/model scope`
- CLI 核心 flags：model/provider/thinking/api-key/system prompt/session/no-tools/no-context-files
- `@file` 参数处理
- session fork / clone / resume / import
- resume search
- compaction summary / branch summary 基础路径
- image paste 与图片文件引用基础路径
- approval accept 后继续调用 `engine.resolveApproval()`
- skills/templates host API 与 TUI `/skill`、`/template`
- 外部 themes/resource loader 基础路径

因此当前行动计划应从“补 wiring”切换为“验证并补齐运行时语义”。

---

## P0 — 质量 Gate 先恢复

### 当前问题

`npm test` 通过，但 `npm run check` 当前失败在 Biome lint/format 阶段，`tsc -b` 没有执行。继续叠加功能前，应先恢复项目基本质量门槛。

已观察到的问题类别：

- unused variables：approval resolution / tool runner 中有未用变量。
- unused imports / `import type`：host runtime 新拆分文件中存在类型导入问题。
- import/export ordering：Biome 要求导入导出排序。
- formatting：host runtime 若干文件未按 Biome 格式化。

### 目标

- `npm run check` 通过。
- `npm test` 继续通过。
- 后续 parity 变更必须带测试，不再只靠源码接线判断完成。

### 涉及文件

- `packages/engine-native/src/state-machine.ts`
- `packages/engine-native/src/tool-runner.ts`
- `packages/host-runtime/src/host/*`
- `packages/host-runtime/src/index.ts`
- `packages/host-tui/src/tui-app.ts`

---

## P1 — Agent Lifecycle Parity

### 当前问题

`packages/host-runtime/src/scheduler.ts` 已经不是最初的极简 loop：它有 retry、approval、`prepareTurn`、steering/followUp/nextTurn 队列和 outer loop。但它仍不是 pi 的 agent loop 等价实现。

pi 的 `agent-loop.ts` / `AgentHarness` 明确建模：

- `agent_start`
- `turn_start`
- `message_start`
- `message_end`
- `turn_end`
- `agent_end`
- `queue_update`
- `save_point`
- `settled`
- failure message emission
- hook error normalization

piko 当前主要把 engine events 透传给 TUI，缺少稳定的 host-level lifecycle contract。

### 目标

在保留 stateless engine 边界的前提下，引入 host-level lifecycle event：

- 每轮 turn 有明确开始/结束事件。
- 用户消息、assistant 消息、tool result 按 host lifecycle 产生可观察事件。
- failure/abort/max steps/context overflow 有一致的终止事件。
- queued messages 被消费时有 queue update。
- run 完成时有 settled/save point 语义，session 写入可预测。

### 建议方案

1. 扩展 host-runtime 自己的 lifecycle event 类型，不强行塞进 engine protocol。
2. 让 `runScheduler()` 负责 host lifecycle，engine 仍只负责 stateless step。
3. `streamHostPrompt()` 将 lifecycle event 转换为 TUI 可消费事件，或新增并行 host event channel。
4. 增加 scheduler 单元测试，覆盖 normal / tool / approval / error / abort / queued follow-up。

### 涉及文件

- `packages/host-runtime/src/scheduler.ts`
- `packages/host-runtime/src/host/run.ts`
- `packages/host-runtime/src/host/types.ts`
- `packages/host-tui/src/app/submit.ts`

---

## P2 — Turn State / prepareNextTurn 等价

### 当前问题

piko 当前 `prepareTurn()` 可以覆盖 model/provider/thinking/settings/tools，但调用模型更接近“每个 engine step 读取一次固定 closure”。pi 的 `prepareNextTurn` 是基于上一轮 assistant message、tool results、context、newMessages 的 turn snapshot 做动态调整。

当前风险：

- model/thinking/tools 能切换，但没有基于 turn result 的统一决策上下文。
- active tools per turn 只有 `toolsOverride`，缺少 pi 的 active tool names / validation 语义。
- context transform / compaction / resource refresh 与 turn preparation 的边界不清晰。

### 目标

建立 `TurnState` / `TurnResult` 级别的 host abstraction：

- turn 开始前生成当前 model/provider/thinking/tools/systemPrompt/resources。
- turn 结束后拿到 assistant message、tool results、session context、new messages。
- 下一 turn 可基于结果更新 model/thinking/tools/context。
- TUI model/thinking/settings/auth 切换统一写入 turn state source。

### 建议方案

1. 将 `buildPrepareTurnFn()` 改为读取 host runtime state，而不是只捕获调用时参数。
2. 在 scheduler 中引入 `prepareNextTurn(previousTurn)` 或等价 callback。
3. 将 model switching、thinking switching、settings reload 收敛到同一个 runtime config source。
4. 为 Ctrl+P/N、`/model`、`/thinking`、`/settings` 后下一轮真实执行模型增加测试。

### 涉及文件

- `packages/host-runtime/src/host/index.ts`
- `packages/host-runtime/src/host/run.ts`
- `packages/host-runtime/src/scheduler.ts`
- `packages/host-tui/src/app/model.ts`
- `packages/host-tui/src/app/commands-ctx.ts`

---

## P3 — Queue Semantics Hardening

### 当前问题

piko 已有 `steer()` / `followUp()` / `nextTurn()`，但语义比 pi 的 `AgentHarness` 弱：

- 没有 phase 校验：pi 中 idle 时不能 steer/followUp。
- 没有 queue mode：pi 支持 steering/followUp queue mode。
- 没有 queue update event。
- 队列 message 只有 text，不支持 image content。
- TUI streaming 中输入当前会调用 `host.steer(t)`，但缺少可见的 queued state。

### 目标

- 队列操作具备明确 phase 语义。
- queue update 可被 TUI 渲染。
- 支持 text + images 的 queued user message。
- follow-up / nextTurn 消费顺序与 pi 一致，并有测试锁定。

### 建议方案

1. 将 queue item 从 `{ text }` 提升为 host user message payload。
2. `PikoHost` 增加 run phase 状态，idle 时拒绝或转化 steer/followUp。
3. scheduler 消费队列时 emit queue update。
4. 为 steering mid-stream、follow-up after completion、nextTurn before prompt 添加测试。

### 涉及文件

- `packages/host-runtime/src/host/index.ts`
- `packages/host-runtime/src/scheduler.ts`
- `packages/host-tui/src/app/init.ts`
- `packages/host-tui/src/app/submit.ts`

---

## P4 — Session Write / Save Point Semantics

### 当前问题

pi `AgentHarness` 有 pending session writes、flush、save point、settled 语义。piko 当前大多在 `runHostPrompt()` / `streamHostPrompt()` 结束后一次性 `saveMessages()`，中途消息和 queued messages 的保存边界不如 pi 明确。

### 目标

- 每个重要 lifecycle 点的 session 写入策略明确。
- tool execution、approval pause、abort、error 时不会丢失已产生消息。
- branch/fork/clone/resume 时不会与正在进行的 run 发生隐式冲突。

### 建议方案

1. 在 host-runtime 中引入 pending session write queue 或等价 save point 抽象。
2. approval pause / abort / error 路径显式 flush。
3. session replacement 前检查 run phase，必要时 abort/flush。
4. 增加恢复测试：中断后打开 session，确认 transcript 完整。

### 涉及文件

- `packages/host-runtime/src/host/run.ts`
- `packages/host-runtime/src/session/session-manager.ts`
- `packages/host-runtime/src/session/session-runtime.ts`
- `packages/host-tui/src/app/session.ts`

---

## P5 — Resource Invocation Polish

### 当前问题

skills/templates 已经从“静态提示词注入”升级为 host/TUI 显式能力，但仍未完全等价：

- skill invocation 没有 pi 的专用 message renderer。
- skill metadata 中 model/tools 相关字段没有成为 turn-state override。
- package-installed skills 未纳入。
- CLI 没有 `--prompt-template` 直接调用入口。

### 目标

- skill/template invocation 结果在 session 中有清晰、可恢复、可渲染的记录。
- skill metadata 可影响当前 turn model/tools/thinking。
- 保留 `.piko/skills` / `.piko/prompts` 的同时，支持安装来源。

### 涉及文件

- `packages/host-runtime/src/skills/*`
- `packages/host-runtime/src/prompts/*`
- `packages/host-runtime/src/host/skills.ts`
- `packages/host-tui/src/chat-view.ts`
- `packages/host-tui/src/commands/handler.ts`
- `packages/cli/src/cli.ts`

---

## P6 — TUI Consistency Polish

### 当前问题

多数 UI 功能已经存在，但仍需确保显示状态与真实 runtime state 不漂移：

- header/footer 的 model、thinking、session 信息应来自同一 runtime source。
- `/login` 后 model registry/provider config refresh 需要验证。
- `/settings` 后 theme/thinking/model scope/compaction/retry 的运行时效果需要测试。
- key hints 仍较粗粒度，但不是主阻塞。

### 目标

- 所有 TUI 状态都能追溯到 host runtime state 或 settings manager。
- slash command、overlay、快捷键共用同一套状态更新路径。
- 关键交互有测试或 smoke case。

### 涉及文件

- `packages/host-tui/src/app/*`
- `packages/host-tui/src/commands/*`
- `packages/host-tui/src/overlays/*`

---

## 建议执行顺序

```text
Phase 0 — Clean Gate
  └── 修复 lint/format/typecheck，保证 npm run check 通过

Phase 1 — Lifecycle Contract
  └── 补 host-level lifecycle events 与测试

Phase 2 — Turn State
  └── 将 prepareTurn 升级为 prepareNextTurn/TurnState 模型

Phase 3 — Queue Semantics
  └── 补 phase 校验、queue update、image queue、消费顺序测试

Phase 4 — Session Writes
  └── 明确 save point / pending writes / abort-error flush

Phase 5 — Resource Polish
  └── skill/template metadata、renderer、CLI invocation

Phase 6 — TUI Consistency
  └── 清理 UI 状态与真实 runtime state 的漂移风险
```

---

## 非目标

以下内容不作为本文 parity 目标：

- 完整复刻 pi 的 extension runtime
- custom provider hooks 的 1:1 API 对齐
- extension message renderer / keyboard shortcut registration 的等价 API
- package manager 的完整功能
- print/json/rpc 模式的完整对齐

这些功能可以继续演进，但不作为判断 `coding-agent + agent core` 是否复刻完成的主标准。
