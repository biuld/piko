# piko - Implementation Gaps & Action Plan

> 基于 2026-06-01 当前代码复核。
>
> 目标口径:复刻 `pi-mono` 的 `coding-agent + agent core` 主线能力;extension runtime 不要求和 pi 等价,应按 piko 的 Host + stateless Engine 架构重新设计。

---

## 总览

```text
piko 现在:  host + engine 架构 ✓ · host-runtime agent core 主线接近等价 ✓ · TUI 常用路径大部分覆盖 ✓ · parallel tools ✓
pi 目标:    coding agent + agent core 行为语义等价
当前差距:  TUI/settings smoke · quality gate warnings · deferred CLI/package/extension/RPC/share/changelog
```

历史文档里的若干 P0/P1 已经完成:

- SettingsManager 接入 CLI/TUI/Host
- ModelRegistry + AuthStorage 基础接线
- runtime model switching 通过 `PikoHost.setConfig()` 影响下一轮请求
- scoped model cycling 与 `/model scope`
- CLI 基础 flags:model/provider/thinking/api-key/system prompt/session/no-tools/no-context-files/prompt-template/skill
- `@file` 参数处理
- session fork / clone / resume / import / tree / rename
- resume search
- compaction summary / branch summary 基础路径
- image paste 与图片文件引用基础路径
- approval accept 后继续调用 `engine.resolveApproval()`
- skills/templates host API 与 TUI `/skill`、`/template`
- host-level lifecycle / queue update / save point / settled
- steer/followUp/nextTurn queue semantics 与 phase guard
- 外部 themes/resource loader 基础路径

因此当前行动计划应从"补 wiring"切换为"补齐 coding-agent 全量 parity 和 pi agent core 深层语义"。

### 本次复核修正

历史文档此前把结论写成"core coding agent 功能基本等价"。这个口径对 `host-runtime` 主循环基本成立,但对 `coding-agent + agent core` 全量 parity 过于乐观。当前更准确的判断:

- ✅ `host-runtime` agent loop、TurnState、queue、session save point、approval resume 已接近 pi agent core 主线。
- ✅ `host-tui` 常用交互路径大部分存在。
- ✅ `engine-native` 已支持 parallel/sequential tool execution 分流。
- 🔶 CLI modes/flags、package resources、extensions、RPC/print/json、`/share`、`/changelog` 已明确暂不处理。

---

## P0 - Quality Gate & Test Environment ✅ done

### 当前状态

- ✅ `npm run check` 退出码通过,但 Biome 仍报告 7 个 unused import/variable 警告。
- ✅ `npm test` 通过:17 test files / 97 tests(无需手动设置 HOME)。
- ✅ `test/setup.ts` 在 vitest setupFiles 中为所有测试创建 writable temp HOME。

### 方案

1. ✅ 已增加 Vitest `setupFiles: ["./test/setup.ts"]`,将 `HOME` 指向 `tmpdir()` 下的临时目录。
2. session-manager 相关测试已有显式 test session dir。
3. CI 中同时跑 `npm run check` 与 `npm test`。

---

## P1 - Rich Lifecycle Contract ✅ done

### 当前状态

`packages/host-runtime/src/loop/agent-loop.ts` 已有完整 host lifecycle:

- ✅ `agent_start` / `turn_start` / `turn_end` / `queue_update` / `save_point` / `settled` / `agent_end` / `failure`
- ✅ `message_start` / `message_update`(含 `isThinking` 标记)/ `message_end`
- ✅ `tool_execution_start` / `tool_execution_update` / `tool_execution_end`
- ✅ Failure path 有 synthetic assistant message emission(`message_start` → `message_end` 后接 `failure`)
- ✅ Steering/follow-up/next-turn 用户消息注入有完整 `message_start` / `message_end` lifecycle

引擎事件通过 `processEngineEvent()` 自动映射为 host lifecycle events,TUI/session/future RPC 可以统一消费 host lifecycle 而不再依赖 raw engine events。

### 涉及文件

- `packages/host-runtime/src/host/lifecycle-events.ts` - 所有 lifecycle event 类型 + `LifecycleMessage`
- `packages/host-runtime/src/loop/agent-loop.ts` - scheduler 主循环、retry、approval resume、save point、settled
- `packages/host-runtime/src/loop/engine-events.ts` - engine event 到 lifecycle event 的映射
- `packages/host-runtime/src/loop/lifecycle.ts` - `emitFailureMessage()` / save point helpers
- `packages/host-runtime/src/host/index.ts` - re-export
- `packages/host-runtime/src/index.ts` - re-export
- `packages/host-runtime/test/lifecycle.test.ts` - 10 tests covering rich lifecycle

---

## P2 - Full Turn State / prepareNextTurn Parity ✅ done

### 当前状态

piko 已有完整 per-turn TurnState snapshot:

- ✅ `TurnState` 类型:每轮构建 messages、systemPrompt、model、provider、thinkingLevel、allTools、activeTools、settings。
- ✅ `createPrepareNextTurn()` 返回 `PrepareTurnFn`(替代旧的 `TurnPreparation` overrides)。
- ✅ `buildDefaultTurnState()` 在无 prepareTurn 回调时提供默认 TurnState。
- ✅ `getSystemPrompt` 回调支持 per-turn system prompt 刷新(PikoHost 已接入)。
- ✅ `HostConfig.tools` 携带工具定义,`TurnState.allTools` 和 `TurnState.activeTools` 均基于此。
- ✅ `TurnResult` 类型已定义（turn 输入输出语义建模完成）。
- ✅ active tools 选择逻辑已实现并加固（skill `tools` metadata → 显式 `ActiveToolsState` → 限制 activeTools）— Phase 3。

### 涉及文件

- `packages/host-runtime/src/turn-state.ts` - `TurnState`、`TurnResult`、`PrepareTurnFn`、`TurnBuildContext`
- `packages/host-runtime/src/loop/agent-loop.ts` - `buildDefaultTurnState()`、integrated into run loop
- `packages/host-runtime/src/host/run.ts` - `createPrepareNextTurn()` with `getSystemPrompt`
- `packages/host-runtime/src/models/config.ts` - `HostConfig.tools`
- `packages/host-runtime/src/host/index.ts` - wiring `getSystemPrompt` into callbacks
- `packages/host-runtime/test/turn-state.test.ts` - 6 tests (include 2 new TurnState snapshot tests)

---

## P3 - Queue Semantics ✅ first pass / 🔶 harden UI

### 当前状态

piko 已补齐第一版 queue semantics:

- ✅ `steer()` / `followUp()` idle 时拒绝。
- ✅ `nextTurn()` idle 时允许。
- ✅ steering/followUp queue mode 存在。
- ✅ queue item 支持 text + images。
- ✅ scheduler 消费队列时 emit `queue_update`(含 message preview)。
- ✅ TUI `doSubmit` 消费 `queue_update`,status line 显示 pending steering/followUp 状态。
- ✅ tests 覆盖 idle phase、during-run queue、default steering consumption。

### 剩余风险

- 🟡 queue_update event 的 message preview 为 truncated text,完整队列 message 仍需从 session state 读取。
- 🟡 queued skill/template expansion 还需要和普通 prompt 路径收敛。

---

## P4 - Session Write / Save Point Semantics ✅ hardened

### 当前状态

piko 已补第一版 save point,并增加了 per-message flush:

- ✅ `runScheduler()` 在 step 边界(`appendMessages` 后)调用 `onMessageFlush` → per-message 持久化
- ✅ `runHostPrompt()` / `streamHostPrompt()` 在 `onSavePoint` 和 `onMessageFlush` 均调用 `saveMessages()`
- ✅ run 结束后仍做 final save
- ✅ session mutation 在 running phase 会被拒绝
- ✅ tests 覆盖 turn boundary incremental save、abort flush、run 中 session mutation reject

### 改进

- ✅ 新增 `onMessageFlush` callback - 每个 step 追加消息后立即 flush,增强 crash resilience
- ✅ 长多工具序列中 crash 最多丢失一个 step 的消息(而不是整个 turn)

---

## P5 - Active Tools & Resource Invocation ✅ hardened

### 当前状态

- ✅ skills/templates 已从"静态提示词注入"升级为 host/TUI 显式能力。
- ✅ skill `model` metadata 会临时覆盖 model。
- ✅ skill `thinking` metadata 会临时覆盖 thinking level。
- ✅ skill `tools` metadata 会临时设置 active tools state(限制当前 turn 可用工具)。
- ✅ `PikoHost.getActiveToolNames()` / `setActiveToolNames()` 公开 API。
- ✅ `createPrepareNextTurn()` 根据显式 `ActiveToolsState` 过滤 `TurnState.activeTools`。
- ✅ `appendActiveToolsChange()` session 持久化。
- ✅ `restoreFromSession()` 恢复 active tools 状态;无 active_tools entry 的 session 会恢复为 all,不会继承旧 session 限制。
- ✅ 空 `active_tools_change` entry 明确表示 clear/all tools。
- ✅ engine-native 已修复 `input.tools` 覆盖语义:undefined 使用 engine defaults,[] 表示显式无工具。
- ✅ CLI 已有 `--prompt-template` 和 `--skill` 启动入口。
- ✅ skill/template invocation 专用 message renderer 已在 TUI 接入。
- 🟡 package-installed skills/prompts/themes/extensions 未纳入。若目标是 coding-agent 全量 parity,这是 package resource 体系缺口,不是纯体验 polish。

### 涉及文件

- `packages/host-runtime/src/turn-state.ts` - `ActiveToolsState` 显式状态
- `packages/host-runtime/src/host/index.ts` - active tools state + skill invocation override
- `packages/host-runtime/src/host/run.ts` - `createPrepareNextTurn()` 显式过滤
- `packages/host-runtime/src/host/restore.ts` - session restore
- `packages/host-runtime/test/resource-invocation.test.ts` - skill tools test
- `packages/host-runtime/test/turn-state.test.ts` - inactive tool / restore clear / no-entry restore tests

---

## P6 - CLI Modes & Flags 🔶 deferred

### 当前状态

piko CLI 当前主要负责解析少量基础参数,建立 `SettingsManager` / `AuthStorage` / `ModelRegistry`,然后启动 TUI:

- ✅ `--model` / `--provider`
- ✅ `--thinking`
- ✅ `--api-key`
- ✅ `--system-prompt` / `--append-system-prompt`
- ✅ `--session` / `--continue` / `--session-dir` / `--name`
- ✅ `--no-context-files` / `--no-tools`
- ✅ `--prompt-template` / `--skill`
- ✅ `--list-models`

### 暂缓项

- 🔶 `--print/-p` 非交互模式。
- 🔶 `--mode text|json|rpc`。
- 🔶 `--models` model cycling scope CLI flag。
- 🔶 `--tools` / `--exclude-tools` / `--no-builtin-tools`。
- 🔶 `--extension` / `--no-extensions`、`--no-skills`、`--no-prompt-templates`、`--theme`、`--no-themes`。
- 🔶 `--export` 直接导出并退出。
- 🔶 model pattern/glob/fuzzy/provider-prefix/`:thinking` shorthand。
- 🔶 RPC mode/client parity。

### 后续方案

1. 先实现 print text mode,复用 `PikoHost.run()` 并输出最终 assistant text。
2. 增加 JSON mode,输出 session id、status、messages、usage/error。
3. 再实现 RPC mode,优先基于 host lifecycle event contract,不要复刻 pi monolithic internals。
4. 补 CLI model/tool/resource flags,通过 `SettingsManager` overrides 和 `ActiveToolsState` 进入 host。

---

## P7 - Tool Execution Semantics ✅ done

### 当前状态

- ✅ tool call 执行、tool result message、approval gating、approval resume 已可用。
- ✅ built-in coding tools 覆盖 read/bash/edit/write/grep/find/ls。
- ✅ 默认 parallel tool execution。
- ✅ `parallelTools: false` 可强制顺序执行。
- ✅ 任一 called tool 标记 `executionMode: "sequential"` 时整批顺序执行。
- ✅ 并行执行时 tool result messages 仍按原 tool call 顺序写入 transcript。

### 已对齐语义

pi agent core 支持:

- 默认 parallel tool execution。
- config 级 `toolExecution === "sequential"` 时顺序执行。
- 任一 tool call 对应工具 `executionMode === "sequential"` 时整批顺序执行。

piko 现在已具备对应语义。协议层使用 `parallelTools?: boolean` 作为 settings 级控制;tool definition 使用 `executionMode?: "parallel" | "sequential"`。

---

## P8 - Slash Command Parity 🟡 partial

### 当前状态

多数常用 TUI 命令已实现,但 command definitions 与 handler 仍不一致。

### 暂缓项

- 🔶 `/share` 在 definitions 中存在,暂不处理。
- 🔶 `/changelog` 在 definitions 中存在,暂不处理。
- 🟡 `/model <search>` 解析 search term 但 selector 未使用。
- 🟡 `/settings` 只覆盖基础设置项,未达到 pi 设置面板完整度。

### 建议方案

1. 把 `/model <search>` 传给 selector 初始过滤。
2. 继续补 `/settings`、`/login` 的 runtime consistency smoke。
3. `/share`、`/changelog` 后续恢复时再设计具体交互。

---

## P9 - Extension Surface / Package Resources 🔶 deferred, but not full parity

### 当前状态

piko 有轻量 TUI extension host:

- register command
- register tool API shape
- UI helpers
- event handlers

但它主要存在于 `host-tui`,还没有成为 host-runtime/engine 边界上的扩展面。`registerTool()` 目前没有接入 engine tool definitions/executors。

### 当前口径

这块先暂缓,不作为 `host-runtime` agent loop 主线等价的同等优先级事项。

- 不要求 piko 复刻 pi extension runtime API。
- 不把 extension runtime 作为当前 host-runtime 主线验收项。
- piko 扩展性应等 core coding agent 功能完整、Host/Engine 边界稳定后,再做 piko-native 设计。
- 但如果验收口径是 `coding-agent + agent core` 全量 parity,package-installed skills/prompts/themes/extensions 和 extension-provided tools 必须作为缺口记录。

### 后续可重新评估的能力

- Host-owned hooks:`before_agent_start`、context transform、resources discovery、session lifecycle。
- Engine/provider hooks:before provider request、before provider payload、after provider response。
- Tool hooks:tool call validation/interception、tool result transform、extension tool executor。
- UI extension APIs:commands、overlays、widgets、key hints、RPC bridge。

### 涉及文件

- `packages/host-runtime/src/host/*`
- `packages/host-runtime/src/resource-loader.ts`
- `packages/host-tui/src/extensions/*`
- `packages/host-tui/src/app/init.ts`
- `packages/engine-native/src/engine.ts`

---

## P10 - TUI Consistency Polish 🟡 partial

### 当前状态

多数 UI 功能已经存在:

- `/model` / `/scoped-models` / `/thinking`
- `/settings`
- `/login` / `/logout`
- `/resume` / `/sessions` / `/tree` / `/fork` / `/clone` / `/new`
- `/skill` / `/template`
- `/compact` / `/export` / `/reload`
- image paste / file argument processing

### 剩余风险

- 🟡 header/footer 的 model、thinking、session 信息应持续来自同一 runtime source。
- 🟡 `/login` 后 model registry/provider config refresh 需要更多 smoke。
- 🟡 `/settings` 后 theme/thinking/model scope/compaction/retry 的运行时效果需要更完整验证。
- 🟡 key hints 仍较粗粒度。
- ✅ OAuth device-code flow 已升级到 RFC 8628 标准（AbortSignal 取消、正确 slow_down 处理、WSL/VM 错误提示）。
- 🟡 queued skill/template expansion 与普通 prompt 路径还需要进一步收敛。
- 🟡 command definitions 中存在未实现命令,需要和 handler 收敛。

### 本次推进

- ✅ Queue visibility - `QueueUpdateEvent` 增加 `steerPreview` / `followUpPreview` / `nextTurnPreview`(truncated message text)
- ✅ TUI `doSubmit` 通过 `onLifecycleEvent` 消费 `queue_update`,在 status line 显示 pending steering/followUp
- ✅ `host-tui` lifecycle wiring 已接通(`onLifecycleEvent` → status line)
- ✅ Skill/template invocation message renderer - `ChatView` 新增 `kind: "skill" | "template"`,渲染 ⚡/📋 前缀
- ✅ `doSubmitStream` / `submitStream` / command handler 传递 `kind` 参数
- ✅ Theme 新增 `skillLabel` / `templateLabel` 颜色
- ✅ Header 显示 active tools 状态(`tools:read,edit`)当工具受限时

### 涉及文件

- `packages/host-tui/src/chat-view.ts` - skill/template message renderer
- `packages/host-tui/src/app/submit.ts` - lifecycle wiring + queue visibility
- `packages/host-tui/src/app/header-footer.ts` - active tools display
- `packages/host-tui/src/commands/handler.ts` - `kind` param for skill/template
- `packages/host-tui/src/theme.ts` - `skillLabel` / `templateLabel`

---

## 建议执行顺序

- **Phase 0 - Test Environment** ✅ — 让 bun test 在沙箱内直接使用可写 HOME/session root
- **Phase 1 - Rich Lifecycle** ✅ — 补 message/tool execution host lifecycle,并让 TUI 消费 host lifecycle
- **Phase 2 - Turn State** ✅ — 建立完整 TurnState/TurnResult,统一 model/thinking/tools/systemPrompt/resources/auth snapshot
- **Phase 3 - Active Tools** ✅ — 支持 active tool names、skill tools metadata、validation、session restore,并修复状态污染
- **Phase 4 - TUI Polish** 🟡 partial — 登录、设置、OAuth、key hints、queue visibility、image details
- **Phase 5 - Compaction Hardening** ✅ — 补 branch summary / compaction 的错误可见性、设置覆盖和边界测试
- **Phase 6 - OAuth Upgrade** ✅ — RFC 8628 标准 polling、AbortSignal cancel、正确 slow_down 处理
- **Phase 7 - Tool Execution Semantics** ✅ — parallel/sequential tool execution 和 per-tool executionMode 已完成
- **Deferred - CLI Modes & Flags** 🔶 — print/json/rpc modes、model/tool/resource flags、model pattern parsing 暂缓
- **Deferred - Slash Commands** 🔶 — /share、/changelog 暂缓; /model \<search\> 可单独处理
- **Deferred - Extension Surface** 🔶 deferred — 等 core coding agent 功能完整后,再设计 piko-native extension surface

---

## 非目标

以下内容不作为当前 parity 的硬性完成标准:

- 完整复刻 pi 的 extension runtime API
- custom provider hooks 的 1:1 API 对齐
- extension keyboard shortcut registration 的等价 API

以下内容已经不应继续列为非目标,而应作为 coding-agent parity 缺口追踪:

- print/json/rpc 模式。
- package-installed skills/prompts/themes/extensions 的发现和加载。
- extension-provided tools 接入 engine tool registry。

扩展性相关 API 形状仍可暂缓。等 piko 的 core coding agent 功能完整后,再按 piko-native 架构重新设计 extension surface;不要为了 1:1 API 兼容牺牲 Host + stateless Engine 边界。
