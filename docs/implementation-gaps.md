# piko — Implementation Gaps & Action Plan

> 基于 2026-05-31 codex review。
> 目标口径已调整：**不追求完整复刻 pi 的 extension runtime**；只关注 `host-runtime` / `host-tui` 对 `pi-mono` 的 `coding-agent + agent core` 主线能力复刻。

---

## 总览：当前状态 vs 目标

```text
piko 现在:  host + engine 架构 ✓  ·  基础会话/TUI/工具链已贯通 ✓  ·  多数表层功能已接上 ✓
pi 目标:    在不照搬 extension 系统的前提下，复刻 coding agent + agent core 主线语义
```

当前判断：

- 已完成的主要是 wiring、session/TUI 基础能力、settings/model/auth/resource 基础设施
- 仍未完成的是 agent loop 语义、运行时模型切换、skills/templates 的宿主调用能力、branch summary 专用流程
- 因此 **piko 还不能算和 pi 的 coding agent + agent core 功能等价**

---

## 已完成但不再作为主阻塞项

以下工作已经基本完成，或不再是 parity 的核心判定标准：

- SettingsManager 接入 CLI/TUI/Host
- ModelRegistry 基础接线
- AuthStorage 基础接线
- CLI 核心 flags
- `@file` 参数处理
- session fork / clone / resume / import
- resume search
- git branch footer
- compaction summary / branch summary renderer
- image paste 与图片文件引用
- approval accept 后继续执行原 tool
- 外部 themes / resource loader
- 轻量扩展系统

说明：

- 轻量扩展系统可继续保留，但**不再要求**与 pi 的完整 extension runtime 等价
- 本文后续阶段不再把 extension parity 作为主线目标

---

## P0 — Runtime Model Switching

### 当前问题

`host-tui` 中的模型切换主要停留在 UI 层：

- `runTui()` 启动时只创建一次 `PikoHost`
- `Ctrl+P/N` 和 `/model` 会更新 `currentModel/currentProviderConfig`
- 但后续请求仍走旧的 `host.config`

这意味着：

- footer/header 显示的模型，可能和真实执行模型不一致
- scoped model cycling 虽已接上列表，但没有真正影响 agent 运行时
- `/login` 后的 auth 变化也不会自动体现在当前 host run 上

### 目标

做到和 pi 一致的运行时语义：

- 模型切换必须真正影响下一轮请求
- provider config / auth 变化必须能在下一轮生效
- TUI 展示状态与实际运行状态一致

### 建议方案

优先采用 host 层显式更新，而不是只改 TUI 局部状态。

可选实现：

1. 给 `PikoHost` 增加 `setModel()` / `setProviderConfig()` / `reconfigure()` 接口
2. 或在 `runScheduler()` 前通过 `prepareTurn()` 注入当前 model/provider/tools/thinking
3. `/login`、`/model`、Ctrl+P/N 都只更新单一的“当前运行配置源”，由 host 在下一 turn 读取

### 涉及文件

- `packages/host-tui/src/tui-app.ts`
- `packages/host-runtime/src/host.ts`
- `packages/host-runtime/src/scheduler.ts`
- `packages/host-runtime/src/models/registry.ts`

---

## P1 — Agent Loop 语义补齐

### 当前问题

`scheduler.ts` 现在是一个简化的 step loop：

- 有 retry
- 有 approval
- 有单次 `prepareTurn`

但还不是 pi 的 agent loop 语义。缺的不是“某个按钮”，而是宿主行为模型：

- 无 steering queue
- 无 follow-up queue
- 无 next-turn 消息队列
- 无 turn 结束后继续消费排队消息的 outer loop
- 无 host 级的 `steer()` / `followUp()` / `nextTurn()` API

### 目标

在不移植 `AgentHarness` 类的前提下，复刻其核心语义：

- 正在 streaming 时，用户消息可排队，而不是只能中断
- agent 可以在 turn 结束后追加 follow-up / next-turn 消息
- 每一 turn 开始前都能动态决定 model / thinking / tools / context

### 最小等价语义

| 语义 | 优先级 | 说明 |
|---|---|---|
| steering queue | P1 | streaming 中用户消息排队，下一 turn 注入 |
| follow-up queue | P1 | agent/host 追加 follow-up 消息 |
| nextTurn preparation | P1 | 每轮动态决定 model/thinking/tools |
| outer loop after agent-end | P1 | agent 将结束时继续检查排队消息 |
| active tools per turn | P2 | 每轮动态切换可用工具 |
| turn lifecycle hooks | P2 | beforeTurn / afterTurn / onError |

### 建议方案

- 不直接复刻 pi 的 `AgentHarness`
- 在 `runScheduler()` 中引入双层循环
- 明确引入三类队列：
  - `steeringQueue`
  - `followUpQueue`
  - `nextTurnQueue`
- 在 host 层暴露：
  - `steer(text, options?)`
  - `followUp(text, options?)`
  - `nextTurn(text, options?)`
- `prepareTurn()` 升级为每轮调用，而不是一次性 override

### 涉及文件

- `packages/host-runtime/src/scheduler.ts`
- `packages/host-runtime/src/host.ts`
- `packages/host-runtime/src/session/`

---

## P2 — Skills / Prompt Templates 从“提示词注入”升级为“宿主能力”

### 当前问题

现在的 skills / prompt templates 主要是：

- 在 system prompt 中列出可用资源
- 让模型“知道它们存在”

但还没有形成 pi 的 host/runtime 调用语义：

- 没有 `skill(name, additionalInstructions?)`
- 没有 `promptFromTemplate(name, args?)`
- TUI 里没有 `/template`
- 也没有稳定的 template invocation 流程

### 目标

把 skills / templates 从“静态提示信息”提升为“宿主可显式调用的能力”。

### 建议方案

1. `PikoHost` 增加显式调用接口
   - `runSkill(name, additionalInstructions?)`
   - `runPromptTemplate(name, args?)`
2. TUI 增加命令入口
   - `/template <name> [args...]`
   - 可选 `/skill <name> [extra instructions]`
3. 保持 system prompt 注入，但不再依赖模型自行理解调用语义

### 涉及文件

- `packages/host-runtime/src/host.ts`
- `packages/host-runtime/src/prompts/`
- `packages/host-runtime/src/skills/`
- `packages/host-tui/src/commands.ts`
- `packages/host-tui/src/tui-app.ts`

---

## P3 — Branch Summary 专用流程补齐

### 当前问题

目前 `branchSummary` 的 renderer 已存在，但 branch 前自动逻辑仍偏粗糙：

- branch 前自动调用的是通用 `compact()`
- 没有稳定走 `branch summarization` 专用流程
- `SettingsManager.branchSummary` 相关配置没有真正成为行为入口

### 目标

使 branch navigation 的行为更接近 pi：

- 分支切换时优先生成 branch summary，而不是复用普通 compaction
- branch summary 与 compaction summary 语义分离
- branch 相关设置真正生效

### 建议方案

1. 引入独立 `maybeBranchSummarizeBeforeNavigation()`
2. 使用 `collectEntriesForBranchSummary()` + `generateBranchSummary()`
3. `branchToEntry()` 中按 branch summary 设置决定是否生成 summary
4. compaction 仍只负责上下文裁剪，不再兼任 branch summary

### 涉及文件

- `packages/host-runtime/src/host.ts`
- `packages/host-runtime/src/compaction/branch-summarization.ts`
- `packages/host-runtime/src/settings/manager.ts`

---

## P4 — TUI 交互一致性收尾

这部分不再是“功能是否存在”，而是“是否与运行时真实状态一致”。

### 当前问题

- header/footer 展示状态与真实运行状态存在漂移风险
- thinking/model 的 UI 切换不一定真正进入 host run
- `/login` 后缺少运行时 rebind / refresh 机制
- key hints 仍较粗粒度，但这不是主阻塞

### 目标

- 所有 TUI 状态都应来自单一运行时源
- 切换模型、thinking、auth 后，下一轮行为可预测
- slash command、overlay、快捷键共用同一套状态更新路径

### 建议方案

- 收敛 model/thinking/auth 到单一“当前 turn config”
- `commands.ts` 不再直接只改局部 UI 状态
- `tui-app.ts` 在 turn 开始前统一从 runtime config 生成执行参数

### 涉及文件

- `packages/host-tui/src/tui-app.ts`
- `packages/host-tui/src/commands.ts`
- `packages/host-tui/src/overlays/login-dialog.ts`

---

## 建议执行顺序

```text
Phase 1 — Runtime Model Switching
  └── 让 TUI 中的模型/认证切换真正影响下一轮运行

Phase 2 — Agent Loop Semantics
  └── 在 scheduler/host 中补齐 steering / follow-up / nextTurn 语义

Phase 3 — Skill / Template Invocation
  └── 把 skills/templates 从 prompt 注入升级为 host/tui 显式能力

Phase 4 — Branch Summary
  └── 将 branch summary 从通用 compaction 里拆出来

Phase 5 — TUI Consistency Polish
  └── 清理 UI 状态与运行时状态不一致的问题
```

---

## 依赖关系

```text
Runtime model switching
  └── 是 TUI parity 的前提

Agent loop semantics
  └── 是后续 steer/follow-up/nextTurn、动态 tools、动态 model 的基础

Skill/template invocation
  └── 依赖稳定的 host turn entrypoints

Branch summary
  └── 可独立推进，但最好放在 agent loop 主线稳定之后
```

---

## 非目标

以下内容不作为本文 parity 目标：

- 完整复刻 pi 的 extension runtime
- custom provider hooks 的 1:1 API 对齐
- message renderer / keyboard shortcut registration 的 extension 等价
- package manager、print/json/rpc 模式

这些功能可以继续演进，但不再作为判断 `coding-agent + agent core` 是否复刻完成的主标准。
