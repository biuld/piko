# piko - Implementation Gaps & Action Plan

> 基于 2026-05-31 当前代码复核。
>
> 目标口径:复刻 `pi-mono` 的 `coding-agent + agent core` 主线能力;extension runtime 不要求和 pi 等价,应按 piko 的 Host + stateless Engine 架构重新设计。

---

## 总览

```text
piko 现在:  host + engine 架构 ✓ · 基础会话/TUI/工具链贯通 ✓ · rich lifecycle ✓ · full TurnState ✓ · active tools hardened ✓
pi 目标:    coding agent + agent core 行为语义等价
当前差距:  TUI/settings/OAuth smoke · compaction hardening · deferred extension design
```

历史文档里的若干 P0/P1 已经完成:

- SettingsManager 接入 CLI/TUI/Host
- ModelRegistry + AuthStorage 基础接线
- runtime model switching 通过 `PikoHost.setConfig()` 影响下一轮请求
- scoped model cycling 与 `/model scope`
- CLI 核心 flags:model/provider/thinking/api-key/system prompt/session/no-tools/no-context-files/prompt-template/skill
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

因此当前行动计划应从"补 wiring"切换为"补齐 pi agent core 深层语义"。

---

## P0 - Quality Gate & Test Environment ✅ done

### 当前状态

- ✅ `npm run check` 通过。
- ✅ `npm test` 通过:15 test files / 67 tests(无需手动设置 HOME)。
- ✅ `test/setup.ts` 在 vitest setupFiles 中为所有测试创建 writable temp HOME。

### 方案

1. ✅ 已增加 Vitest `setupFiles: ["./test/setup.ts"]`,将 `HOME` 指向 `tmpdir()` 下的临时目录。
2. session-manager 相关测试已有显式 test session dir。
3. CI 中同时跑 `npm run check` 与 `npm test`。

---

## P1 - Rich Lifecycle Contract ✅ done

### 当前状态

`packages/host-runtime/src/scheduler.ts` 已有完整 host lifecycle:

- ✅ `agent_start` / `turn_start` / `turn_end` / `queue_update` / `save_point` / `settled` / `agent_end` / `failure`
- ✅ `message_start` / `message_update`(含 `isThinking` 标记)/ `message_end`
- ✅ `tool_execution_start` / `tool_execution_update` / `tool_execution_end`
- ✅ Failure path 有 synthetic assistant message emission(`message_start` → `message_end` 后接 `failure`)
- ✅ Steering/follow-up/next-turn 用户消息注入有完整 `message_start` / `message_end` lifecycle

引擎事件通过 `processEngineEvent()` 自动映射为 host lifecycle events,TUI/session/future RPC 可以统一消费 host lifecycle 而不再依赖 raw engine events。

### 涉及文件

- `packages/host-runtime/src/host/lifecycle-events.ts` - 所有 lifecycle event 类型 + `LifecycleMessage`
- `packages/host-runtime/src/scheduler.ts` - `processEngineEvent()` + `emitFailureMessage()` + `emitUserMessageLifecycle()`
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
- `packages/host-runtime/src/scheduler.ts` - `buildDefaultTurnState()`、integrated into run loop
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
- 🟡 package-installed skills 未纳入(低优先级)。

### 涉及文件

- `packages/host-runtime/src/turn-state.ts` - `ActiveToolsState` 显式状态
- `packages/host-runtime/src/host/index.ts` - active tools state + skill invocation override
- `packages/host-runtime/src/host/run.ts` - `createPrepareNextTurn()` 显式过滤
- `packages/host-runtime/src/host/restore.ts` - session restore
- `packages/host-runtime/test/resource-invocation.test.ts` - skill tools test
- `packages/host-runtime/test/turn-state.test.ts` - inactive tool / restore clear / no-entry restore tests

---

## P6 - Extension Surface 🔶 deferred

### 当前状态

piko 有轻量 TUI extension host:

- register command
- register tool API shape
- UI helpers
- event handlers

但它主要存在于 `host-tui`,还没有成为 host-runtime/engine 边界上的扩展面。`registerTool()` 目前没有接入 engine tool definitions/executors。

### 当前口径

这块先暂缓,不作为当前和 pi 行为一致的同等优先级事项。

- 不要求 piko 复刻 pi extension runtime API。
- 不把 extension runtime 作为当前 pi 行为一致性验收项。
- piko 扩展性应等 core coding agent 功能完整、Host/Engine 边界稳定后,再做 piko-native 设计。

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

## P7 - TUI Consistency Polish 🟡 partial

### 当前状态

多数 UI 功能已经存在:

- `/model` / `/models` / `/thinking`
- `/settings`
- `/login` / `/logout`
- `/resume` / `/sessions` / `/tree` / `/fork` / `/clone` / `/new`
- `/skill` / `/template`
- `/compact` / `/export` / `/reload`
- image paste / file argument processing

### 剩余风险

- header/footer 的 model、thinking、session 信息应持续来自同一 runtime source。
- `/login` 后 model registry/provider config refresh 需要更多 smoke。
- `/settings` 后 theme/thinking/model scope/compaction/retry 的运行时效果需要更完整验证。
- key hints 仍较粗粒度。
- OAuth UI 尚未达到 pi 等价。
- queued skill/template expansion 与普通 prompt 路径还需要进一步收敛。

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

```text
Phase 0 - Test Environment ✅
  └── 让 npm test 在沙箱内直接使用可写 HOME/session root

Phase 1 - Rich Lifecycle ✅
  └── 补 message/tool execution host lifecycle,并让 TUI 消费 host lifecycle

Phase 2 - Turn State ✅
  └── 建立完整 TurnState/TurnResult,统一 model/thinking/tools/systemPrompt/resources/auth snapshot

Phase 3 - Active Tools ✅
  └── 支持 active tool names、skill tools metadata、validation、session restore,并修复状态污染

Phase 4 - TUI Polish 🟡 partial
  └── 登录、设置、OAuth、key hints、queue visibility、image details

Phase 5 - Compaction Hardening 🟡
  └── 补 branch summary / compaction 的错误可见性、设置覆盖和边界测试

Deferred - Extension Surface 🔶 deferred
  └── 等 core coding agent 功能完整后,再设计 piko-native extension surface
```

---

## 非目标

以下内容不作为当前 parity 的硬性完成标准:

- 完整复刻 pi 的 extension runtime API
- custom provider hooks 的 1:1 API 对齐
- extension keyboard shortcut registration 的等价 API
- package manager 的完整功能
- print/json/rpc 模式的完整对齐

扩展性相关能力暂缓。等 piko 的 core coding agent 功能完整后,再按 piko-native 架构重新设计 extension surface;不要把 pi extension runtime parity 放入当前主线验收。
