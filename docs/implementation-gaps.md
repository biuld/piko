# piko — Implementation Gaps & Action Plan

> 基于 2026-05-31 codex review，从 missing-features.md 提炼出的高优先级行动项。
> **Phase 0 ✅ | Phase 1 ✅ | Phase 2 ✅ | Phase 3 🟡 | Phase 4 ✅ | Phase 5 ⬜**

---

## 总览：当前状态 vs 目标

```
piko 现在:  host + engine 架构 ✓  ·  coding agent 雏形 ✓  ·  核心模块已贯通 ✓
pi 目标:    完整 coding agent + agent core 功能等价
```

Phase 0 解决了 "not wired" 问题；Phase 1 补齐了 Agent Core 基础语义。以下为剩余阶段。

---

## P0 — 贯通核心模块 ✅ 已完成

### P0.1 接入 SettingsManager 到 CLI/TUI 启动流程 ✅

**文件：**
- `packages/cli/src/cli.ts`
- `packages/host-tui/src/tui-app.ts`
- `packages/host-runtime/src/settings/manager.ts`（已完整）

**要做的事：**
1. CLI 启动时 `SettingsManager.create(cwd)`，从 settings 读取 default model/provider/theme/thinking
2. TUI 启动时同样从 SettingsManager 读取默认值
3. CLI flags override SettingsManager（`settings.applyOverrides(...)`）
4. Compaction 阈值从 SettingsManager 读取（去掉 `host.ts` 硬编码）
5. `host.ts` 的 `PikoHost` 接收 `SettingsManager` 实例

### P0.2 用 ModelRegistry 替代直接 findModel() ✅

**文件：**
- `packages/cli/src/cli.ts`
- `packages/host-tui/src/tui-app.ts`
- `packages/host-runtime/src/models/registry.ts`（已完整）
- `packages/host-runtime/src/models/loader.ts`（将被替代）

**要做的事：**
1. CLI 创建 `ModelRegistry`（注入 `AuthStorage`），用 `registry.resolve()` 替代 `findModel()`
2. TUI 创建 `ModelRegistry`，`listScopedModels()` 用于 model cycling
3. `/login` 保存 key → `AuthStorage.set()` → `ModelRegistry` 自动生效（因为 registry 内部读取 authStorage）
4. `cycleModelForward/Backward` 改为使用 `listScopedModels()`（当 SettingsManager 有 `enabledModels` 时）

### P0.3 接入 Prompt Templates ✅

**文件：**
- `packages/host-runtime/src/prompts/prompt-templates.ts`（已完整）
- `packages/host-runtime/src/host.ts`
- `packages/host-tui/src/tui-app.ts`

**要做的事：**
1. `host.ts` 的 `buildEnhancedSystemPrompt()` 调用 `loadPromptTemplates({ cwd })`，传入 `buildSystemPrompt()`
2. TUI 启动时加载模板，注册为 `/template` slash command
3. CLI 增加 `--prompt-template` 参数
4. `system-prompt.ts` 的 `buildSystemPrompt()` 增加 `promptTemplates` 参数和注入逻辑

---

## P1 — Agent Core 语义补齐 ✅ 已完成

### P1.1 重构 Scheduler 为 Agent Loop ✅

**参考文件：**
- `/Users/biu/Projects/pi-mono/packages/agent/src/agent-loop.ts`
- `/Users/biu/Projects/pi-mono/packages/agent/src/harness/agent-harness.ts`

**当前文件：** `packages/host-runtime/src/scheduler.ts`

**缺失的语义：**

| 语义 | 优先级 | 说明 |
|---|---|---|
| steering queue | P1 | streaming 时用户消息排队（Ctrl+S），不 abort 当前 run |
| follow-up queue | P1 | agent 产生的 follow-up 消息队列 |
| nextTurn | P1 | `prepareNextTurn()` — 动态切换 model/thinking/tools |
| retry continuation | P2 | 错误后重试，带 exponential backoff |
| event-driven turn lifecycle | P2 | hook 事件（beforeTurn / afterTurn / onError） |
| active tools per turn | P2 | per-turn 动态调整可用 tools |
| queued messages after agent-end | P3 | agent 完成后继续处理排队消息 |

**建议方案：**
- 不直接移植 pi 的 `AgentHarness` 类（架构不同）
- 在 `scheduler.ts` 中增加 steering/followUp/nextTurn 队列
- `executeStep()` 前调用 `prepareNextTurn()` 决定 model/thinking/tools
- 所有队列处理后 `continue` 而非 `completed`

### P1.2 修复 Approval：accept 后执行原 tool ✅

**文件：**
- `packages/engine-native/src/state-machine.ts` — `runApprovalResolution()`
- `packages/engine-native/src/tool-runner.ts` — `executeToolCalls()`
- `packages/engine-native/src/tools/registry.ts` — tool definitions

**要做的事：**
1. `runApprovalResolution()` accept 分支：返回 `status: "continue"` + 原 `assistantMessage`（或新的继续信号）
2. `executeToolCalls()` 记住已审批但未执行的 tool call，在 resolution 后执行
3. 内置 destructive tools（edit/write/bash）设置 `metadata.requiresApproval: true`
4. `allowApprovals` 设置真正影响是否检查审批

### P1.3 完善 Compaction 自动化 ✅

**文件：**
- `packages/host-runtime/src/host.ts`
- `packages/host-runtime/src/compaction/`

**要做的事：**
1. `host.ts` 的 `compact()` 和 `maybeCompact()` 从 `SettingsManager` 读取阈值
2. branch navigation 时自动触发 branch summary
3. compaction 调用的 model 可配置（当前用 `this.config.model`）
4. 增强错误处理：compaction 失败时不应 crash 主流程

---

## P2 — CLI 功能补齐 ✅ 已完成

### P2.1 CLI 参数扩展 ✅

**pi 参考：** pi CLI 约 20+ 参数

**当前文件：** `packages/cli/src/cli.ts`

**要增加的参数：**

| 参数 | 优先级 |
|---|---|
| `--api-key <key>` | P1 |
| `--thinking <level>` | P1 |
| `--system-prompt <text>` | P2 |
| `--append-system-prompt <text>` | P2 |
| `--session-dir <path>` | P2 |
| `--skills <path>` | P3 |
| `--prompt-template <path>` | P2 |
| `--no-context-files` | P3 |
| `--name <session-name>` | P2 |
| `--tools` / `--no-tools` | P3 |

### P2.2 接入 file-processor（@file 语法） ✅

**文件：** `packages/cli/src/file-processor.ts`（已实现） → `packages/cli/src/cli.ts`

在 prompt 构建前调用 file-processor 解析 `@path`，将文件内容追加/前置到 user message。

---

## P3 — TUI Parity ✅ 部分完成

### P3.1 组件补齐 🟡

| 组件 | 状态 | 要做的事 |
|---|---|---|
| branch summary renderer | ⬜ | 新增组件，渲染 branch summary message |
| compaction summary renderer | ⬜ | 新增组件，渲染 compaction summary |
| bash execution 专用 renderer | 🔶 | tool-block 中检测 bash tool，专用渲染 |
| tool call progress/status | 🟡 | streaming 时显示 tool 执行进度 |
| image display | ⬜ | Kitty/ITerm 协议 inline 图片 |
| theme loader (外部 JSON) | ⬜ | 从 `.piko/themes/` 加载外部 theme |
| session search | 🟡 | resume-selector 增加搜索 |

### P3.2 Model cycling 接入 scoped models

**文件：** `packages/host-tui/src/tui-app.ts`

`cycleModelForward/Backward` 当前使用 `listAvailableModels()` 的全部模型。改为：如果有 Settings 的 `enabledModels`，则只在 scoped list 内循环。绑定 Ctrl+P 快捷键。

---

## P4 — 扩展与资源系统 ✅ 已完成

### P4.1 资源系统统一化 ✅

**要做的事：**
1. 实现 `host-runtime/src/resource-loader.ts` — 统一发现 `.piko/skills/` `.piko/prompts/` `.piko/themes/`
2. `PikoHost.create()` 调用 resource loader 汇集所有资源
3. 各 loader（skills/prompts/themes）改为从 resource loader 获取路径

### P4.2 扩展系统增强 ✅

**文件：** `packages/host-tui/src/extensions/`

**要做的事：**
1. 增加 custom tool registration（extension 可注册 EngineTool）
2. 增加 event hooks（onMessage / onToolCall / onTurnEnd 等）
3. 增加 message renderer（extension 可自定义 message 渲染）
4. 增加 keyboard shortcut registration

---

## 建议执行顺序

```
Phase 1 (P0) — 贯通  ［1-2 周］
  ├── P0.1: SettingsManager 接入 CLI/TUI
  ├── P0.2: ModelRegistry 替代 findModel
  └── P0.3: Prompt Templates 接入

Phase 2 (P1) — 核心语义 ［2-3 周］
  ├── P1.1: Scheduler → Agent Loop（steering/followUp/nextTurn）
  ├── P1.2: Approval 修复
  └── P1.3: Compaction 自动化

Phase 3 (P2) — CLI 补齐 ［1 周］
  ├── P2.1: CLI 参数扩展
  └── P2.2: file-processor 接入

Phase 4 (P3) — TUI Parity ［2-3 周］
  └── P3.1 + P3.2: 组件补齐 + scoped cycling

Phase 5 (P4) — 扩展/资源 ［2-3 周］
  ├── P4.1: Resource Loader
  └── P4.2: 扩展系统增强
```

---

## 依赖关系

```
SettingsManager (P0.1) ──→ Compaction 自动化 (P1.3)
                        ──→ Model cycling (P3.2)
                        ──→ Settings Selector 生效

ModelRegistry (P0.2)   ──→ Auth 贯通 (P0.2)
                        ──→ Model cycling (P3.2)

Agent Loop (P1.1)      ──→ 后续所有 turn-level 功能
```
