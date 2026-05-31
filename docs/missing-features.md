# piko — pi coding agent 功能缺口分析

> **状态标记：** ✅ done · 🟡 partial · 🔶 stub · ❌ not wired · ⬜ missing
>
> **最后更新：** 2026-05-31（基于 codex 全量 review）

本文档列出 piko 相对于 pi coding agent（`@earendil-works/pi-coding-agent`）的功能实现状态。

---

## piko Package 架构回顾

| Package | 职责 |
|---|---|
| `engine-protocol` | 纯类型定义：EngineInput / EngineEvent / EngineStepResult / StatelessEngine 接口 |
| `engine-native` | 进程内 stateless engine：LLM 调用 + tool 执行状态机 |
| `engine-remote` | JSON-RPC client，对接远程 engine server |
| `host-runtime` | Host 层核心：scheduler、session 存储、model loading、approval controller |
| `host-tui` | 终端 UI：chat view、editor、overlays、tool block 渲染 |
| `cli` | CLI 入口：参数解析、模式分发、help |

---

## 🔴 核心运行模式

### 1. HTML 导出 ✅ done

**pi 现状：** `core/export-html/` 将 session JSONL 转换为自包含 HTML 文件。

**piko 现状：** `host-runtime/src/export-html/` 已实现（export.ts），有 ANSI→HTML 转换、tool 渲染、syntax highlighting。

---

### [NEW] Agent Core 语义 🟡 partial → ✅ done (retry, prepareTurn, error handling)

**pi 现状：** `packages/agent/src/agent-loop.ts` + `harness/agent-harness.ts` 实现完整 agent loop：
- steering queue / follow-up queue / next-turn queue
- prepareNextTurn（动态 model/thinking/tools）
- retry / continuation
- hook/event 驱动的 turn lifecycle
- active tools per turn
- resources invocation 语义
- agent-end 后继续处理 queued messages

**piko 现状：** `host-runtime/src/scheduler.ts` 是极简 while 循环：
```
while totalSteps < maxSteps:
  engine.executeStep()
  append messages
  maybe approval
  continue/completed/error
```
缺少 steering/followUp/nextTurn、prepareNextTurn、retry continuation、hook/event 驱动的 turn lifecycle、active tools per turn、resources invocation。

**放置位置：** `host-runtime/src/scheduler.ts` — 重构为接近 agentLoop 的语义

---

### [NEW] Approval 修复 ✅ done

**pi 现状：** accept 后继续执行被审批的 tool call。

**piko 现状：** `engine-native/src/state-machine.ts` 的 `runApprovalResolution()` 对 `accept` 只返回 `completed`，没有继续执行原 tool call。内置 tool 定义（registry.ts）也没有设置 `requiresApproval`。

**放置位置：** `engine-native/src/state-machine.ts` + `engine-native/src/tools/registry.ts`

---

## 🟡 资源与配置

### 2. Skills 系统 🟡 partial

**pi 现状：** `core/skills.ts` 从 `.pi/skills/` 和已安装 package 加载 `.md` 文件，解析 YAML frontmatter，合并成 prompt。

**piko 现状：**
- ✅ `host-runtime/src/skills/` 已实现（loader / formatter / types）
- ✅ `host.ts` 的 `buildEnhancedSystemPrompt()` 已调用 `loadSkills()`
- ❌ 未接入：package-installed skills、CLI flags 控制 skills、skill invocation message、skill model/tools metadata

**放置位置：** `host-runtime/src/skills/`

### 3. Prompt Templates ❌ not wired

**pi 现状：** `core/prompt-templates.ts` 支持 `--prompt-template` 或 `.pi/prompts/`。

**piko 现状：**
- ✅ `host-runtime/src/prompts/prompt-templates.ts` 已实现 loader + 参数替换
- ✅ `PikoHostCreateOptions` 有 `promptTemplates?: PromptTemplate[]`
- ❌ 主流程（host.ts / tui-app.ts）没有调用 `loadPromptTemplates()`，没有传入 system prompt 构建
- ❌ 没有 `/template` slash command
- ❌ CLI 没有 `--prompt-template` 参数

**放置位置：** `host-runtime/src/prompts/prompt-templates.ts`

### 4. Context Files ✅ done

**pi 现状：** `ResourceLoader.loadProjectContextFiles()` 加载 `AGENTS.md` / `CLAUDE.md`。

**piko 现状：**
- ✅ `host-runtime/src/prompts/context-files.ts` 已实现
- ✅ `host.ts` 的 `buildEnhancedSystemPrompt()` 已调用 `loadContextFiles()`
- ✅ system-prompt.ts 正确注入

### 5. Settings Manager ❌ not wired

**pi 现状：** `core/settings-manager.ts` 分层配置（default → user → project → CLI flags）。

**piko 现状：**
- ✅ `host-runtime/src/settings/manager.ts` 已完整实现（分层合并、持久化、mutator）
- ❌ CLI（`cli.ts`）不使用 SettingsManager — 直接用 `findModel()` / `listAvailableModels()`
- ❌ TUI（`tui-app.ts`）不使用 SettingsManager — 启动时直接 `findModel()`
- ❌ SettingsManager 的 `defaultProvider` / `defaultModel` / `defaultThinkingLevel` / `theme` / `compaction` 等对启动流程无影响
- ❌ compaction 阈值在 `host.ts` 硬编码，不读取 SettingsManager

**放置位置：** `host-runtime/src/settings/manager.ts` — 需要接入 CLI 和 TUI 启动流程

### 6. Model Registry ❌ not wired

**pi 现状：** `core/model-registry.ts` 集中管理 provider → model 映射，集成 auth。

**piko 现状：**
- ✅ `host-runtime/src/models/registry.ts` 已实现 `ModelRegistry`（含 scopedModels、auth 集成）
- ❌ CLI 和 TUI 仍使用 `model-loader.ts` 的 `findModel()` / `listAvailableModels()`，不经过 ModelRegistry
- ❌ `/login` 保存的 API key 不影响 model resolution（因为用的是 findModel 而非 registry）
- ❌ scoped models 没有用于 model cycling（cycleModelForward/Backward 使用全部模型）

**放置位置：** `host-runtime/src/models/registry.ts` — 需要替代 `model-loader.ts` 在 CLI/TUI 中的调用

### 7. Auth System 🟡 partial

**pi 现状：** `AuthStorage` 多 backend（file / in-memory），runtime API key injection。

**piko 现状：**
- ✅ `host-runtime/src/auth/storage.ts` 已完整实现（FileAuthStorage + InMemoryAuthStorage + 优先级解析）
- ✅ `host-tui/src/overlays/login-dialog.ts` 已实现
- ❌ login dialog 保存的 key 不接入 model resolution（因为 CLI/TUI 不用 ModelRegistry）
- ❌ CLI 没有 `--api-key` 参数

**放置位置：** `host-runtime/src/auth/storage.ts`

### 8. Advanced Compaction 🟡 partial → ✅ done (settings-wired, auto branch summary)

**pi 现状：** 完整 compaction 子系统：LLM-based branch summarization、智能 cut point、可配置 settings。

**piko 现状：**
- ✅ `host-runtime/src/compaction/` 已实现（compaction.ts / branch-summarization.ts / messages.ts / utils.ts）
- ✅ `host.ts` 有 `compact()` 和 `maybeCompact()`
- ❌ 阈值硬编码在 `host.ts`：`{ enabled: true, reserveTokens: 16384, keepRecentTokens: 20000 }`
- ❌ 不读取 SettingsManager 的 compaction 配置
- ❌ branch navigation 时没有自动 branch summary
- ❌ compaction 错误处理较弱

**放置位置：** `host-runtime/src/compaction/`

---

## 🟡 TUI 功能

### 9. Theme 系统 ✅ done (load() wired, settings theme applied on startup)

**pi 现状：** 多个内置 theme（JSON 文件），运行时动态切换（Ctrl+T / `/theme`），file watcher 自动重载。

**piko 现状：**
- ✅ `host-tui/src/theme/manager.ts` 已实现 ThemeManager
- ✅ `/theme` slash command 已实现
- ✅ `switchTheme()` 已接入 tui-app.ts
- ❌ 只有内置 theme，无外部 JSON theme 加载（`.piko/themes/` 未实现）
- ❌ 无 file watcher

**放置位置：** `host-tui/src/theme/`

### 10. Diff 渲染 ✅ done

**pi 现状：** `components/diff.ts` — unified diff 彩色渲染。

**piko 现状：** `host-tui/src/components/diff.ts` 已实现。已接入 tool-block。

### 11. Thinking Selector UI ✅ done

**pi 现状：** think level 选择器 overlay（off/low/medium/high/xhigh）。

**piko 现状：** `host-tui/src/overlays/thinking-selector.ts` 已实现。已接入 tui-app.ts `/thinking` 和 `openThinkingSelector()`。

### 12. Login Dialog 🟡 partial

**pi 现状：** 交互式 OAuth 登录流程 UI。

**piko 现状：**
- ✅ `host-tui/src/overlays/login-dialog.ts` 已实现
- ✅ `/login` slash command 已接入
- ❌ 保存的 key 不接入 model resolution（not wired 问题）
- ❌ 无 OAuth flow（只有 API key 输入）

**放置位置：** `host-tui/src/overlays/login-dialog.ts`

### 13. Settings Selector UI 🟡 partial

**pi 现状：** tree-structured settings 编辑 overlay。

**piko 现状：**
- ✅ `host-tui/src/overlays/settings-selector.ts` 已实现
- ✅ `/settings` slash command 已接入
- ❌ SettingsManager 改动不反映到运行时（因为 SettingsManager 本身 not wired）

**放置位置：** `host-tui/src/overlays/settings-selector.ts`

### 14. Keybinding Hints 🟡 partial

**pi 现状：** `keybinding-hints.ts` — 根据当前 context 动态显示可用快捷键。

**piko 现状：**
- ✅ `host-tui/src/components/key-hints.ts` 已有 `getContextHints("normal" | "streaming" | "overlay")`
- ✅ tui-app.ts 的 footer 已接入 key hints
- ❌ context 切换粗糙（只有 3 种），pi 有更细粒度的 per-context hints
- ❌ 没有 `KeybindingsManager`

**放置位置：** `host-tui/src/components/key-hints.ts`

### 15. Session Picker with Search ✅ done (search input + applyFilter in resume-selector)

**pi 现状：** 带全文搜索的 session 列表 overlay。

**piko 现状：**
- ✅ `host-tui/src/overlays/resume-selector.ts` 已实现基础列表
- ❌ 无搜索输入框
- ❌ 没有使用 `session-types.ts` 已有的 `getSearchableText()`

**放置位置：** `host-tui/src/overlays/resume-selector.ts`

### 16. Model Scoping 🔶 stub

**pi 现状：** `resolveModelScope()` 解析 scoped model 列表（Ctrl+P 循环）。

**piko 现状：**
- ✅ `ModelRegistry` 有 `scopedModels` 和 `listScopedModels()`
- ❌ ModelRegistry not wired（见 #6）
- ❌ `cycleModelForward/Backward` 使用 `listAvailableModels()` 的全部模型
- ❌ 无 Ctrl+P 快捷键

**放置位置：** `host-runtime/src/models/registry.ts` + `host-tui/src/tui-app.ts`

### 17. Image 支持 🔶 stub

**pi 现状：** clipboard 粘贴、PNG→WebP、`@file` 图片、TUI inline 显示。

**piko 现状：**
- ✅ `host-runtime/src/utils/image.ts` 已实现基础 resize/转换
- ❌ 无 clipboard 读取
- ❌ 无 `@file` 图片输入
- ❌ 无 TUI inline 图片渲染
- ❌ message content 图片路径未贯通

### 18. File Argument Processing（@file 语法） ✅ done

**pi 现状：** `cli/file-processor.ts` 解析 `@path` 参数，支持 glob 展开。

**piko 现状：**
- ✅ `cli/src/file-processor.ts` 已实现
- ❌ `cli.ts` 主流程没有调用 file-processor

**放置位置：** `cli/src/cli.ts` — 在 prompt 构建前调用 `file-processor.ts`

---

## 🟢 基础设施

### 19. Git 集成 🟡 partial

**pi 现状：** footer 中显示 git branch。

**piko 现状：**
- ✅ `host-runtime/src/utils/git.ts` 已实现
- ❌ `host.ts` 没有暴露 `getGitBranch()`
- ❌ footer 没有显示 git branch

**放置位置：** `host-runtime/src/utils/git.ts` + `host-tui/src/components/footer.ts`

### 20. Telemetry / Timing ✅ done

**pi 现状：** `core/timings.ts` — `time(label)` / `printTimings()`。

**piko 现状：** `host-runtime/src/utils/timings.ts` 已实现。

### 21. System Prompt 构建 ✅ done

**pi 现状：** 统一构建 system prompt（skills + context + tools + guidelines）。

**piko 现状：** `host-runtime/src/prompts/system-prompt.ts` 已实现完整 builder。`host.ts` 的 `buildEnhancedSystemPrompt()` 已接入 skills + context files + tools + guidelines。

### 22. HTTP Dispatcher ✅ done

**pi 现状：** 全局配置 HTTP 连接空闲超时。

**piko 现状：** `host-runtime/src/utils/http-dispatcher.ts` 已实现。

---

## 🚫 暂不考虑

以下功能暂不列入开发计划。

### 23. Print 模式 ⬜ missing

**pi 现状：** `runPrintMode()` 支持 `text` 和 `json` 两种输出格式。

**放置位置：** `host-runtime/src/print-runner.ts` + `cli/src/cli.ts`

### 24. JSON 输出 ⬜ missing

**pi 现状：** `--json` / `-j` 标志，输出 JSONL 格式。

**放置位置：** `host-runtime/src/print-runner.ts`

### 25. RPC 模式 ⬜ missing

**pi 现状：** `runRpcMode()` stdin/stdout JSON-RPC 2.0 服务端。

**放置位置：** `cli/src/rpc-server.ts` 或新建 `packages/host-rpc/`

### 26. Package Manager ⬜ missing

**pi 现状：** `pi install/uninstall/update/list <package>`。

**放置位置：** `cli/src/package-manager.ts`

### 27. Piped stdin ⬜ missing

**pi 现状：** `readPipedStdin()` 检测 piped stdin。

**放置位置：** `cli/src/cli.ts`

### 28. Version Check / Self-update ⬜ missing

**pi 现状：** `utils/version-check.ts` 检测最新版本。

**放置位置：** `cli/src/version-check.ts`

### 29. Offline Mode ⬜ missing

**pi 现状：** `--offline` flag / `PI_OFFLINE` env。

**放置位置：** `cli/src/cli.ts` + `engine-native`

---

## ⚡ 低优先级

### 30. Resource Loader ⬜ missing

**pi 现状：** `core/resource-loader.ts` 统一发现 extensions / skills / prompts / themes / context files。

**放置位置：** `host-runtime/src/resource-loader.ts`

### 31. Session Migrations ⬜ missing

**pi 现状：** `migrations.ts` 检测旧格式 session 文件并自动升级。

**放置位置：** `host-runtime/src/session/migrations.ts`

### 32. CLI 参数完整性 🟡 partial (core flags done: --name, --no-context-files, --no-tools, --api-key, --thinking, --system-prompt etc.)

**pi 现状：** 完整 CLI 参数约 20+ 个。

**piko 现状：** `cli/src/cli.ts` 只支持 `-m`、`--provider`、`-c`、`--session`、`--list-models`、`--help`，缺少大部分 pi CLI 参数。

**放置位置：** `cli/src/args.ts` + `cli/src/cli.ts`

---

## [NEW] 扩展系统

原文档将扩展系统列为"暂不考虑"。但 codex review 指出：如果目标是"复刻 pi 功能"，扩展系统不能忽略。

### 33. Extensions Runtime 🟡 partial

**pi 现状：** extensions loader / runner / wrapper、event hooks、custom tools、custom providers、message renderers、keyboard shortcuts、CLI flags、package manager integration。

**piko 现状：**
- ✅ `host-tui/src/extensions/` 有轻量 ExtensionHost（支持 registerCommand、select/confirm/input dialogs、widget slots、status slots、custom footer/editor/working indicator）
- ✅ `tui-app.ts` 已接入 extension loading
- ❌ 不等价于 pi 的完整 extension runtime：无 event hooks、无 custom tools、无 custom providers、无 message renderers、无 keyboard shortcuts、无 package manager 集成

**放置位置：** `host-tui/src/extensions/` + `host-runtime/src/extensions/`

---

## 总结

| 状态 | 数量 | 说明 |
|---|---|---|
| ✅ done | 6 | HTML Export、Context Files、Diff、Thinking UI、Timings、System Prompt、HTTP Dispatcher |
| 🟡 partial | 10 | Agent Core、Skills、Auth、Compaction、Theme、Login Dialog、Settings UI、Key Hints、Session Search、Extensions |
| 🔶 stub | 4 | Model Scoping、Images、@file、CLI Args |
| ❌ not wired | 3 | **Prompt Templates、Settings Manager、Model Registry** |
| ⬜ missing | 8 | Print、JSON Output、RPC、Package Manager、Piped stdin、Version Check、Offline Mode、Resource Loader、Session Migrations |
| **总计** | **32+1** | |

### 最关键的 3 个 "not wired" 问题

这三个模块自身实现完整，但没有接入主流程，导致大量功能不可用：

1. **SettingsManager** — 未接入 CLI/TUI 启动 → 所有 settings 持久化无意义
2. **ModelRegistry** — CLI/TUI 用 `findModel()` 而非 registry → auth 不贯通、scoped models 不生效
3. **Prompt Templates** — loader 已实现但主流程不调用 → 模板功能不可用
