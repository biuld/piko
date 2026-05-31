# piko — pi coding agent 功能缺口分析

> **状态标记：** ✅ done · 🟡 partial · 🔶 weak/needs hardening · ❌ missing
>
> **最后更新：** 2026-05-31（基于当前 `host-runtime` / `host-tui` / `cli` 代码复核）

本文档列出 piko 相对于 `pi-mono` 的 `coding-agent + agent core` 主线能力状态。目标不是 1:1 复制 pi 的完整 extension runtime，而是在保留 host + stateless engine 架构的前提下，复刻 coding agent 的用户可见能力与 agent core 行为语义。

---

## 当前结论

```text
piko 当前:  基础 Host/TUI/session/tools/settings/model/auth 已贯通
pi 目标:    agent lifecycle、turn state、queue、session write、resource invocation 语义更完整
结论:       表层功能多数已补齐，但还不能算 agent core 等价
```

当前主要缺口不再是“某个功能完全没接上”，而是运行时语义和工程质量：

- piko 的 `scheduler.ts` 已有 retry、approval、prepareTurn、steering/followUp/nextTurn 队列和 outer loop，但仍是 `EngineStepResult` 层面的简化状态机。
- pi 的 `agent-loop.ts` / `AgentHarness` 有更完整的 `agent_start` / `turn_start` / `message_start` / `message_end` / `turn_end` / `agent_end` lifecycle、phase 校验、queue update、hook error normalization、pending session write、save point、settled 语义。
- `npm test` 通过，但 `npm run check` 当前失败在 Biome lint/format，说明新增接线还没有通过项目质量 gate。

---

## Package 架构回顾

| Package | 职责 |
|---|---|
| `engine-protocol` | 纯类型定义：`EngineInput` / `EngineEvent` / `EngineStepResult` / `StatelessEngine` |
| `engine-native` | 进程内 stateless engine：LLM 调用 + tool 执行状态机 |
| `engine-remote` | JSON-RPC client，对接远程 engine server |
| `host-runtime` | Host 层核心：scheduler、session、settings、auth、models、skills、prompts、compaction |
| `host-tui` | 终端 UI：chat view、editor、overlays、tool block、theme、commands |
| `cli` | CLI 入口：参数解析、model/settings/auth 接线、TUI 启动 |

---

## 核心运行语义

### Agent Core 语义 🟡 partial

**pi 现状：** `packages/agent/src/agent-loop.ts` + `harness/agent-harness.ts` 实现完整 agent loop：

- steering queue / follow-up queue / next-turn queue
- `prepareNextTurn` 动态 model / thinking / tools / context
- retry / continuation
- hook/event 驱动的 turn lifecycle
- active tools per turn
- resources invocation 语义
- `agent_end` 前后继续处理 queued messages
- phase 校验、queue update、pending session writes、save point、settled event

**piko 现状：**

- ✅ `host-runtime/src/scheduler.ts` 已有双层 loop、retry、approval、`prepareTurn`、steering/followUp/nextTurn 队列。
- ✅ `PikoHost` 已暴露 `steer()` / `followUp()` / `nextTurn()`。
- 🟡 仍缺 pi 等价 lifecycle event 语义，当前 streaming event 主要来自 engine/tool 层。
- 🟡 `prepareTurn` 目前只覆盖 model/provider/thinking/settings/tools，缺少 pi 的 `prepareNextTurn` 上下文快照和 transform 语义。
- 🟡 队列 API 缺少 phase 校验、queue mode、queue update event 和 async error handling。
- ❌ 缺少 pending session writes / save point / settled 这类 harness 级 session 写入语义。

**主要位置：** `packages/host-runtime/src/scheduler.ts`、`packages/host-runtime/src/host/index.ts`、`packages/host-runtime/src/host/run.ts`

### Approval 修复 🟡 partial

**pi 现状：** accept 后继续执行被审批的 tool call。

**piko 现状：**

- ✅ `engine-native` 已有 `resolveApproval()` 路径。
- ✅ `scheduler.ts` 在 `awaiting_approval` 后会调用 `engine.resolveApproval()`。
- 🔶 当前 `npm run check` 暴露 approval 相关实现中仍有未用变量，说明这块实现需要清理和补测试。

**主要位置：** `packages/engine-native/src/state-machine.ts`、`packages/engine-native/src/tool-runner.ts`

---

## 资源与配置

### Settings Manager ✅ done

**piko 现状：**

- ✅ `SettingsManager` 已实现 layered settings。
- ✅ CLI 启动已创建 `SettingsManager` 并应用 CLI overrides。
- ✅ TUI/Host 已接收 settings manager。
- ✅ compaction/retry/theme/thinking/model scope 等配置已有运行时入口。

**剩余风险：** `/settings` 修改后的运行时刷新仍需要继续做一致性测试，尤其是 auth/model/theme/thinking 的单一状态源。

### Model Registry ✅ done

**piko 现状：**

- ✅ `ModelRegistry` 已集成 auth storage 和 scoped models。
- ✅ CLI 使用 registry resolve model。
- ✅ TUI model cycling 优先使用 `modelRegistry.listScopedModels()`。
- ✅ TUI 切模型会调用 `host.setConfig()`，真实影响下一轮请求。

**剩余风险：** `/login` 后的当前 host/provider refresh 需要更多集成测试覆盖。

### Auth System 🟡 partial

**piko 现状：**

- ✅ `AuthStorage` 已实现 file/in-memory/runtime API key 优先级。
- ✅ CLI 支持 `--api-key`。
- ✅ TUI `/login` 可保存 API key。
- 🟡 OAuth device-code 基础函数存在，但 TUI 登录流程仍以 API key 为主，未达到 pi 交互式 OAuth UI 等价。

### Skills 系统 🟡 partial

**piko 现状：**

- ✅ `.piko/skills/*.md` loader、formatter、system prompt 注入已实现。
- ✅ `PikoHost.runSkill()` / `streamSkill()` 已实现。
- ✅ TUI `/skill` 已接入。
- 🟡 package-installed skills、skill model/tools metadata、skill invocation 专用 message 渲染尚未等价。

### Prompt Templates ✅ done for host/tui

**piko 现状：**

- ✅ `.piko/prompts/*.md` loader、参数替换、system prompt 注入已实现。
- ✅ `PikoHost.runPromptTemplate()` / `streamPromptTemplate()` 已实现。
- ✅ TUI `/template` 已接入。
- 🟡 CLI 还没有 `--prompt-template` 直接入口，但 TUI/Host 主线已可用。

### Context Files ✅ done

**piko 现状：**

- ✅ 加载 `AGENTS.md` / `CLAUDE.md`。
- ✅ 支持 `--no-context-files`。
- ✅ system prompt 注入已接通。

### Compaction / Branch Summary 🟡 partial

**piko 现状：**

- ✅ compaction 子系统已实现。
- ✅ `PikoHost.compact()` / `maybeCompact()` 已接 settings。
- ✅ branch navigation 已有 `generateAutoBranchSummary()` + `branchWithSummary()` 路径。
- 🟡 branch summary 与普通 compaction 的边界、错误可见性、设置覆盖仍需要测试和收敛。

---

## TUI 功能

### Theme 系统 ✅ done

- ✅ ThemeManager、`/theme`、settings theme startup 已接入。
- ✅ 外部 theme/resource loader 已有基础实现。
- 🟡 file watcher 自动重载不是当前主线阻塞。

### Diff 渲染 ✅ done

- ✅ unified diff 彩色渲染已接入 tool block。

### Thinking Selector ✅ done

- ✅ `/thinking` 和 selector overlay 已接入。
- ✅ `host.setThinkingLevel()` 会影响下一轮请求。

### Login Dialog 🟡 partial

- ✅ `/login` 和 API key 输入已接入。
- 🟡 OAuth UI 未等价。
- 🟡 登录后 model/provider refresh 需要集成测试保证。

### Settings Selector 🟡 partial

- ✅ `/settings` 已接入。
- 🟡 设置修改到运行时状态的一致性需要继续硬化。

### Keybinding Hints 🟡 partial

- ✅ 已有 normal/streaming/overlay 基础 hints。
- 🟡 还没有 pi 的细粒度 keybinding manager。

### Session Picker / Search ✅ done

- ✅ resume selector 已有搜索输入与过滤。

### Model Scoping ✅ done

- ✅ settings scoped models、`/model scope`、Ctrl+P/N cycling 已接入。
- ✅ 切换后更新 host config。

### Image 支持 🟡 partial

- ✅ image utility、clipboard paste、`@file` 图片引用、TUI image component 已有基础实现。
- 🟡 与 pi 的图片压缩/转换/inline 展示细节仍未完全验证等价。

### File Argument Processing ✅ done

- ✅ `@file` 参数处理已实现并在 TUI submit 路径调用。

---

## 当前质量 Gate

最近一次复核结果：

- ✅ `npm test` 通过：9 个 test files / 30 个 tests。
- ❌ `npm run check` 失败：Biome lint/format 阶段失败，`tsc -b` 未执行。

需要优先修复：

- `engine-native/src/state-machine.ts` 未用变量。
- `engine-native/src/tool-runner.ts` 未用变量。
- `host-runtime/src/host/*` import type、导入排序、格式化问题。
- `host-runtime/src/index.ts`、`host-tui/src/tui-app.ts` export 排序/格式化问题。

---

## 下一步优先级

1. 清理 lint/format，让 `npm run check` 重新通过。
2. 为 runtime model switching、steer/followUp/nextTurn、skill/template、branch summary 增加 host-runtime 级测试。
3. 将 scheduler lifecycle 提升到接近 pi agent loop：turn events、message events、queue update、settled/save point。
4. 收敛 `prepareTurn` 为更接近 pi `prepareNextTurn` 的 turn-state 机制。
5. 补齐 TUI 状态一致性测试，确保 header/footer/current host config 不漂移。
