# piko — pi coding agent 功能缺口分析

本文档列出 piko 相对于 pi coding agent（`@earendil-works/pi-coding-agent`）尚未实现的功能，并说明每块缺失在当前 piko package 架构中应放置的位置。

> 扩展系统（event hooks / 自定义 tool / provider registration / keyboard shortcuts / CLI flags / message renderer）因架构不同不列入本文。

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

### 1. HTML 导出

**pi 现状：** `core/export-html/` 将 session JSONL 转换为自包含 HTML 文件，包含 ANSI→HTML 转换、tool 调用渲染、syntax highlighting、样式。

**放置位置：** `host-runtime/src/export-html.ts` — 纯数据转换（无 TUI 依赖），依赖已有的 session JSONL 解析

- `host-runtime/package.json` 可新增 optional dependency：`highlight.js`

---

## 🟡 资源与配置

### 2. Skills 系统

**pi 现状：** `core/skills.ts` 从 `.pi/skills/` 和已安装 package 加载 `.md` 文件，解析 YAML frontmatter（name/description/model/tools），合并成 prompt 注入到 system prompt 的 `## Skills` section。

**放置位置：** `host-runtime/src/skills/`

```
host-runtime/src/skills/
  loader.ts      — 从 .piko/skills/ 目录扫描 .md，解析 frontmatter
  formatter.ts   — 格式化为 system prompt 注入文本
  types.ts       — Skill / SkillFrontmatter 类型
```

- `host.ts` 在构建 system prompt 时调用 skills loader
- Host 负责 system prompt 组装（engine 不管）

### 3. Prompt Templates

**pi 现状：** `core/prompt-templates.ts` 支持自定义 system prompt 模板文件，用户可通过 `--prompt-template` 或 `.pi/prompts/` 加载。

**放置位置：** `host-runtime/src/prompt-templates.ts`

- 从 `.piko/prompts/` 加载模板
- `PikoHostCreateOptions` 新增 `promptTemplate?: string` 选项
- Host 在构建 system prompt 时用模板替换默认 prompt

### 4. Context Files（pi-context.txt / pi-context.md）

**pi 现状：** `ResourceLoader.loadProjectContextFiles()` 加载项目根目录的 `pi-context.txt` 或 `pi-context.md`，注入到 system prompt。

**放置位置：** `host-runtime/src/context-files.ts`

- 在 session cwd 下查找 `.piko/context.txt` / `.piko/context.md`（或沿用 pi 命名）
- Host 在构建 system prompt 时注入

### 5. Settings Manager

**pi 现状：** `core/settings-manager.ts` 管理分层配置（default → user config → project config → CLI flags），覆盖：compaction、image auto-resize、retry、theme、model defaults、package sources、TUI 行为等。

**放置位置：** `host-runtime/src/settings/`

```
host-runtime/src/settings/
  manager.ts     — SettingsManager 类，分层合并
  defaults.ts    — 默认值常量
  types.ts       — Settings 类型定义
  loader.ts      — 从 .piko/config.yaml 或 .piko/settings.json 加载
```

- `model-config.ts` 的 `HostConfig` 可合并进 settings
- `PikoHostCreateOptions` 新增 `settings?: Partial<Settings>`

### 6. Model Registry

**pi 现状：** `core/model-registry.ts` 集中管理所有 provider → model 映射，支持动态注册/卸载 provider，解析 API keys（env / authStorage / runtime injection），输出可用模型列表。

**piko 现状：** `model-loader.ts` 直接调用 `@earendil-works/pi-ai` 的 `getProviders()` / `getModels()`，无持久化注册。

**放置位置：** `host-runtime/src/model-registry.ts`

- 扩展 `model-loader.ts` 或替代之
- 承载 provider configurations（OAuth、custom API URLs、extra headers）
- `PikoHost.create()` 初始化时用 registry 替代直接 `findModel()`

### 7. Auth System

**pi 现状：** `AuthStorage` 支持 API key 和 OAuth credentials 的持久化存储，多 backend（file / in-memory），runtime API key injection。

**放置位置：** `host-runtime/src/auth/`

```
host-runtime/src/auth/
  storage.ts     — AuthStorage 类
  backends/
    file.ts      — ~/.piko/auth.json
    memory.ts    — 内存 backend（测试/CLI --api-key）
  oauth.ts       — OAuth flow helpers
```

- `model-registry.ts` 通过 auth storage 解析 API keys
- `host-tui` 新增 OAuth login 流程（见 TUI 组件部分）

### 8. Advanced Compaction

**pi 现状：** 完整的 compaction 子系统（`core/compaction/`）：
- LLM-based branch summarization（当用户在 tree 中 navigate 时自动总结）
- 智能 cut point 检测（在 tool result 边界切割）
- 可配置的 compaction settings（threshold / keep turns / model）
- 自定义 compaction instructions

**piko 现状：** `scheduler.ts` 中有一个极简版 `compactSession()`：当 token 超过 threshold 时直接截断旧消息并插入一条文本标记。

**放置位置：** `host-runtime/src/compaction/`

```
host-runtime/src/compaction/
  compaction.ts        — 核心 compaction 逻辑
  branch-summary.ts    — tree navigation 时的分支总结
  token-estimator.ts   — token 估算（已有简单版，可增强）
  types.ts             — CompactionSettings 等类型
```

- `scheduler.ts` 的 `compactSession()` 替换为调用 compaction 模块
- 需要在 `EngineRunSettings`（engine-protocol）或 `HostConfig` 中新增 `CompactionSettings`

---

## 🟡 TUI 功能

### 9. Theme 系统

**pi 现状：** `modes/interactive/theme/` 支持多个内置 theme（通过 JSON 文件定义颜色），运行时动态切换（Ctrl+T 或 `/theme`），file watcher 自动重载外置 theme 文件。

**piko 现状：** `theme.ts` 只有一个硬编码 theme。

**放置位置：** `host-tui/src/theme/`

```
host-tui/src/theme/
  themes/
    default.ts    — 内置主题
    dark.ts
    light.ts
  loader.ts       — 从 .piko/themes/ 加载外部 JSON 主题
  manager.ts      — 主题切换、watcher（可选）
```

- 新增 `/theme` slash command
- `tui-app.ts` 的 `getTheme()` 改为从 theme manager 获取

### 10. Diff 渲染

**pi 现状：** `components/diff.ts` — `renderDiff()` 将 unified diff 渲染为带颜色的 Terminal 组件（+++ / --- / @@），用于 edit 工具调用的结果展示。

**piko 现状：** `chat-view.ts` 不渲染 diff；tool block 只显示原始结果文本。

**放置位置：** `host-tui/src/components/diff.ts`

- `tools/tool-block.ts` 检测 edit 工具结果中的 diff 格式，自动调用 diff renderer
- 依赖：可引入 `diff` npm 包做 parsing，或自己解析 unified diff

### 11. Thinking Selector UI

**pi 现状：** `components/thinking-selector.ts` — 独立的 think level 选择器 overlay（off/low/medium/high/xhigh）。

**piko 现状：** 只有 `/thinking [level]` slash command。

**放置位置：** `host-tui/src/overlays/thinking-selector.ts`

- 类似 `model-selector.ts` 的结构
- 绑定快捷键或 slash command 触发

### 12. Login Dialog / OAuth Selector

**pi 现状：** `components/login-dialog.ts` + `components/oauth-selector.ts` — 交互式 OAuth 登录流程 UI。

**放置位置：** `host-tui/src/overlays/login-dialog.ts`

- 依赖 `host-runtime` 的 auth system（见 #7）
- 新增 `/login` slash command

### 13. Settings Selector UI

**pi 现状：** `components/settings-selector.ts` + `components/settings-callbacks.ts` — TUI settings 编辑 overlay（tree-structured settings list）。

**放置位置：** `host-tui/src/overlays/settings-selector.ts`

- 依赖 `host-runtime` 的 Settings Manager（见 #5）
- 新增 `/settings` slash command

### 14. Keybinding Hints 动态更新

**pi 现状：** `components/keybinding-hints.ts` — 根据当前 context 动态显示可用快捷键。

**piko 现状：** 无。

**放置位置：** `host-tui/src/components/key-hints.ts`（已有文件，可扩展）

- 接收 `KeybindingsManager` 或 context，动态渲染当前可用快捷键

### 15. Session Picker with Search

**pi 现状：** `components/session-selector.ts` + `components/session-selector-search.ts` — 带全文搜索的 session 列表 overlay。

**piko 现状：** `overlays/resume-selector.ts` 只有简单列表，无搜索。

**放置位置：** `host-tui/src/overlays/resume-selector.ts`（扩展现有文件）

- 增加搜索输入框
- 依赖 `SessionManager.listAll()` 的 `searchableText` 字段（`session-types.ts` 已有 `getSearchableText()`）

### 16. Model Scoping

**pi 现状：** `core/model-resolver.ts` — `resolveModelScope()` 从配置的 model patterns 解析出 scoped model 列表（Ctrl+P 循环），支持 thinking level 绑定。

**放置位置：** `host-runtime/src/model-scope.ts`

- `PikoHostCreateOptions` 或 Settings 新增 `scopedModels: string[]`
- TUI 的 `cycleModelForward/Backward` 在 scoped list 内循环

### 17. Image 支持

**pi 现状：** 支持 clipboard 粘贴图片（PNG → WebP 转换 + resize），`@file` 加载图片，图片作为 tool result / message content 传递，TUI 内 inline 显示（Kitty/ITerm 协议）。

**放置位置：**

| 模块 | Package |
|---|---|
| 图片 resize / 格式转换 | `host-runtime/src/utils/image.ts` |
| Clipboard 读取 | `host-runtime/src/utils/clipboard.ts`（需 optional native binding） |
| TUI inline 图片渲染 | `host-tui/src/components/image.ts`（需 `@earendil-works/pi-tui` 支持） |
| Message content 携带图片 | `engine-protocol` — `Message` 的 `content` 类型已支持 `ImageContent`（来自 pi-ai） |

### 18. File Argument Processing（@file 语法）

**pi 现状：** `cli/file-processor.ts` — 解析命令行中的 `@path` 参数，读取文件内容并附加到 prompt（支持文本和图片，带 glob 展开）。

**放置位置：** `cli/src/file-processor.ts`

- `cli.ts` 在构建 prompt 前调用，将文件内容追加到 user message

---

## 🟢 基础设施

### 19. Git 集成

**pi 现状：** `utils/git.ts` + `footer-data-provider.ts` — 检测当前 git branch，在 footer 中显示。

**放置位置：** `host-runtime/src/utils/git.ts`

- `PikoHost` 新增 `getGitBranch(): Promise<string | undefined>`
- `host-tui/src/components/footer.ts` 在 footer 中展示

### 20. Telemetry / Timing

**pi 现状：** `core/timings.ts` — `time(label)` / `printTimings()` 定位启动性能瓶颈。

**放置位置：** `host-runtime/src/utils/timings.ts`

- 在 `PikoHost.create()` 各阶段打点
- `cli.ts` 在 `PI_STARTUP_BENCHMARK` 环境变量下输出

### 21. System Prompt 构建

**pi 现状：** `core/system-prompt.ts` 统一构建 system prompt，聚合：skills、context files、prompt templates、model instructions、tool 描述、guidelines。

**piko 现状：** `host.ts` 的 `buildDefaultSystemPrompt()` 只做最简单拼接。

**放置位置：** `host-runtime/src/system-prompt.ts`

- 替代 `PikoHost.buildDefaultSystemPrompt()`
- 汇聚 skills、context files、prompt templates、tools 的 prompt snippet
- 形成一致的 `## Guidelines` / `## Available Tools` / `## Skills` / `## Project Context` 结构

### 22. HTTP Dispatcher / Idle Timeout

**pi 现状：** `core/http-dispatcher.ts` 全局配置 HTTP 连接空闲超时。

**放置位置：** `engine-native` — engine 在创建 LLM caller 时配置

---

## 🚫 暂不考虑

以下功能暂不列入开发计划。

### 23. Print 模式（非交互文本输出）

**pi 现状：** `runPrintMode()` 支持 `text` 和 `json` 两种输出格式，完整运行 agent loop 并将结果输出到 stdout，返回 exit code。

**放置位置（如需实现）：** `cli` + `host-runtime`

- `host-runtime/src/print-runner.ts` — 基于 `runScheduler` 构建纯 stdout 输出器，支持 text/json 两种 format
- `cli/src/cli.ts` — `-p` 参数分发到 print mode

### 24. JSON 输出

**pi 现状：** `--json` / `-j` 标志，输出 JSONL 格式，每行一个完整的 turn result。

**放置位置（如需实现）：** `host-runtime/src/print-runner.ts` — 同 Print 模式，作为 format 选项

### 25. RPC 模式（JSON-RPC Server）

**pi 现状：** `runRpcMode()` 通过 stdin/stdout 运行 JSON-RPC 2.0 协议服务端，暴露 `run` / `list_models` / `list_sessions` 等方法，支持 streaming events。

**放置位置（如需实现）：** `cli` 或新建 `packages/host-rpc/`

- 方案 A：`cli/src/rpc-server.ts` — CLI 直接启动 RPC server
- 方案 B：新建 `packages/host-rpc/` — 独立 package，将 Host 封装为 JSON-RPC 服务端

### 26. Package Manager

**pi 现状：** `core/package-manager.ts` + `package-manager-cli.ts` 支持 `pi install/uninstall/update/list <package>`，从 git repos 安装 extensions/skills/themes。

**放置位置（如需实现）：** `cli/src/package-manager.ts`

- 独立于 Host runtime，纯 CLI 工具
- 管理 `.piko/packages/` 目录

### 27. Piped stdin

**pi 现状：** `main.ts` 的 `readPipedStdin()` — 检测 piped stdin，将内容作为 prompt 的一部分（自动切换到 print 模式）。

**放置位置（如需实现）：** `cli/src/cli.ts`

- 检测 `!process.stdin.isTTY` 时读取 stdin

### 28. Version Check / Self-update

**pi 现状：** `utils/version-check.ts` 检查最新版本并提示升级；Windows 有自更新辅助。

**放置位置（如需实现）：** `cli/src/version-check.ts`

### 29. Offline Mode

**pi 现状：** `--offline` flag / `PI_OFFLINE` env 禁用网络请求。

**放置位置（如需实现）：**

- `cli/src/cli.ts` — 解析 `--offline` flag
- `engine-native` — engine 不发起 LLM 调用
- `host-runtime` — 跳过 version check

---

## ⚡ 低优先级

以下功能建议后续迭代时再实现。

### 30. Resource Loader（自动发现）

**pi 现状：** `core/resource-loader.ts` 统一从多个 source 发现并加载 extensions / skills / prompts / themes / context files，去重，报告错误。

**放置位置：** `host-runtime/src/resource-loader.ts`

- 扫描 `.piko/` 下各子目录：`skills/` `prompts/` `context/`
- `PikoHost.create()` 在初始化时调用，汇集所有资源路径
- 与 skills loader、prompt templates loader、context files loader 配合

### 31. Session Migrations

**pi 现状：** `migrations.ts` — 检测旧格式 session 文件并自动升级。

**放置位置：** `host-runtime/src/session/migrations.ts`

- `SessionManager.open()` 加载时检测 version 字段并执行迁移

### 32. CLI 参数完整性

**pi 现状：** 完整 CLI 参数包括 `--fork` / `--resume` / `--continue` / `--models` / `--provider` / `--tools` / `--no-tools` / `--extensions` / `--skills` / `--prompt-templates` / `--themes` / `--export` / `--session-dir` / `--thinking` / `--api-key` / `--name` / `--no-session` / `--no-context-files` / `--system-prompt` / `--append-system-prompt` 等。

**放置位置：** `cli/src/args.ts` — 新建专用参数解析模块

- `cli/src/cli.ts` 中 args 解析逻辑提取到独立文件
- 支持 `--model provider/model` 格式

---

## 总结

| 层级 | 数量 | 功能 |
|---|---|---|
| 🔴 核心运行模式 | 1 | HTML Export |
| 🟡 资源与配置 | 7 | Skills、Prompt Templates、Context Files、Settings、Model Registry、Auth、Compaction |
| 🟡 TUI 功能 | 10 | Theme、Diff、Thinking UI、Login、Settings UI、Key Hints、Session Search、Model Scope、Images、@file |
| 🟢 基础设施 | 4 | Git、Timings、System Prompt、HTTP Dispatcher |
| 🚫 暂不考虑 | 7 | Print、JSON Output、RPC Server、Package Manager、Piped stdin、Version Check、Offline Mode |
| ⚡ 低优先级 | 3 | Resource Loader、Session Migrations、CLI Args |
| **总计** | **32** | |

大部分缺失功能属于 **Host 层**（host-runtime + host-tui），与 stateless engine 解耦架构兼容。Skills / Prompts / Context Files / System Prompt 构建均属于 Host 的职责范畴（engine 只接收最终的 messages list）。需跨层改动的主要是 Compaction（需 engine-protocol 新增 settings 字段）和 Image 支持（需确认 engine protocol 的 content 类型是否已完备）。
