# piko - pi coding agent 功能缺口分析

> **状态标记:** ✅ done · 🟡 partial · 🔶 weak/needs hardening · ❌ missing
>
> **最后更新:** 2026-05-31(基于当前 `host-runtime` / `host-tui` / `cli` 源码与测试复核)

本文档记录 piko 相对于 `pi-mono` 的 `coding-agent + agent core` 主线能力状态。piko 的目标是复刻 pi 的用户可见能力和 agent core 行为语义,同时保持 Host + stateless Engine 架构边界;不要求 1:1 复刻 pi 的完整 extension runtime。

---

## 当前结论

```text
piko 当前:  Host/TUI/session/tools/settings/model/auth 主线已贯通,rich lifecycle / TurnState / active tools 已完成并加固
pi 目标:    coding agent + agent core 行为语义等价
结论:       core coding agent 功能基本等价,TUI 体验接近 pi
```

和早期历史文档相比,当前状态已经明显前进:

- ✅ `scheduler.ts` 已有 retry、approval resume、`prepareTurn`、steering/followUp/nextTurn 队列、host lifecycle、save point、settled。
- ✅ `PikoHost` 已有 phase 校验、runtime model/thinking 切换、skills/templates 显式调用、session mutation idle guard。
- ✅ `host-runtime` 已新增 lifecycle / queue / turn-state / save-point / resource invocation / TUI consistency 测试。
- ✅ `npm run check` 当前通过。
- ✅ `npm test` 当前通过:15 files / 67 tests(无需手动 HOME)。

当前主要缺口已经从"基础 wiring"转为"pi agent core 深层语义在 piko 架构中的对应实现":

- ✅ Rich lifecycle(message_start / message_update / message_end / tool_execution_* / failure message)已实现。
- ✅ Full TurnState snapshot per turn(替代旧的 TurnPreparation overrides)。
- ✅ Active tools(skill `tools` metadata 限制 turn 可用工具)已接通,并修复 session restore / clear 语义污染。
- ✅ TUI lifecycle wiring + queue visibility + skill/template renderer + active tools header + image dimensions。

---

## Package 架构回顾

| Package | 职责 |
|---|---|
| `engine-protocol` | 纯类型定义:`EngineInput` / `EngineEvent` / `EngineStepResult` / `StatelessEngine` |
| `engine-native` | 进程内 stateless engine:LLM 调用 + tool 执行状态机 |
| `engine-remote` | JSON-RPC client,对接远程 engine server |
| `host-runtime` | Host 层核心:scheduler、session、settings、auth、models、skills、prompts、compaction |
| `host-tui` | Terminal UI:chat view、editor、overlays、tool block、theme、commands、轻量 extension host |
| `cli` | CLI 入口:参数解析、model/settings/auth 接线、TUI 启动 |

---

## 核心运行语义

### Agent Core 语义 ✅ done

**pi 现状:** `packages/agent/src/agent-loop.ts` + `harness/agent-harness.ts` 明确建模:

- `agent_start` / `turn_start` / `message_start` / `message_update` / `message_end` / `turn_end` / `agent_end`
- steering queue / follow-up queue / next-turn queue
- `prepareNextTurn` 动态 model / thinking / tools / context
- hook/event 驱动的 context transform、provider request、tool interception
- active tools per turn
- resources invocation 语义
- phase 校验、queue update、pending session writes、save point、settled

### Agent Core 语义 ✅ done

**piko 现状:**

- ✅ Rich lifecycle:`message_start`、`message_update`(含 `isThinking`)、`message_end`、`tool_execution_start`、`tool_execution_update`、`tool_execution_end`、failure message emission。
- ✅ Full TurnState:每轮构建 `TurnState` snapshot(messages、systemPrompt、model、provider、thinkingLevel、allTools、activeTools、settings),替代旧的 per-field `TurnPreparation` overrides。
- ✅ `HostConfig.tools` + `buildDefaultTurnState()` + `createPrepareNextTurn()` 支持 per-turn system prompt rebuild。
- ✅ `PikoHost` 已暴露 `steer()` / `followUp()` / `nextTurn()`,并在 idle 时拒绝 steer/followUp。
- ✅ queue item 已支持 text + images。
- ✅ save point 会触发 per-turn session save,session mutation 会在 run 中被拒绝。
- ✅ active tools 使用显式 `ActiveToolsState` 建模,避免 `undefined` / `[]` / missing session entry 的语义混淆。

**主要位置:** `packages/host-runtime/src/scheduler.ts`、`packages/host-runtime/src/host/index.ts`、`packages/host-runtime/src/host/run.ts`、`packages/host-runtime/src/host/lifecycle-events.ts`

### Approval 修复 ✅ done

**pi 目标:** accept 后继续执行被审批的 tool call。

**piko 现状:**

- ✅ `engine-native` 已有 `resolveApproval()` 路径。
- ✅ `scheduler.ts` 在 `awaiting_approval` 后会调用 `engine.resolveApproval()`。
- ✅ `npm run check` 已通过,早期 unused variable / lint 问题已清理。

**主要位置:** `packages/engine-native/src/state-machine.ts`、`packages/engine-native/src/tool-runner.ts`、`packages/host-runtime/src/scheduler.ts`

---

## 资源与配置

### Settings Manager ✅ done

- ✅ `SettingsManager` 已实现 layered settings。
- ✅ CLI 启动已创建 `SettingsManager` 并应用 CLI overrides。
- ✅ TUI/Host 已接收 settings manager。
- ✅ compaction/retry/theme/thinking/model scope 等配置已有运行时入口。
- 🟡 `/settings` 修改后所有运行时状态是否完全不漂移,仍需要继续做端到端 smoke。

### Model Registry ✅ done

- ✅ `ModelRegistry` 已集成 auth storage 和 scoped models。
- ✅ CLI 使用 registry resolve model。
- ✅ TUI model cycling 优先使用 `modelRegistry.listScopedModels()`。
- ✅ TUI 切模型会调用 `host.setConfig()`,真实影响下一轮请求。

### Auth System ✅ done

- ✅ `AuthStorage` 已实现 file/in-memory/runtime API key 优先级。
- ✅ CLI 支持 `--api-key`。
- ✅ TUI `/login` 可保存 API key。
- 🟡 OAuth device-code 基础函数和 overlay 存在,但交互流程未达到 pi OAuth UI 等价。

### Skills 系统 ✅ done

- ✅ `.piko/skills/*.md` loader、formatter、system prompt 注入已实现。
- ✅ `PikoHost.runSkill()` / `streamSkill()` 已实现。
- ✅ TUI `/skill` 已接入。
- ✅ skill `model` / `thinking` metadata 调用时会临时覆盖 runtime state。
- ✅ skill `tools` metadata 调用时会临时设置 active tools state(限制当前 turn 工具)。
- ✅ active tools change 可 session 持久化(`active_tools_change` entry)+ restore。
- ✅ skill/template invocation 专用 message renderer 已接入 TUI。
- 🟡 package-installed skills 未纳入(低优先级)。

### Prompt Templates ✅ done for host/tui/cli

- ✅ `.piko/prompts/*.md` loader、参数替换、system prompt 注入已实现。
- ✅ `PikoHost.runPromptTemplate()` / `streamPromptTemplate()` 已实现。
- ✅ TUI `/template` 已接入。
- ✅ CLI 已有 `--prompt-template` 启动入口。

### Context Files ✅ done

- ✅ 加载 `AGENTS.md` / `CLAUDE.md`。
- ✅ 支持 `--no-context-files`。
- ✅ system prompt 注入已接通。

### Compaction / Branch Summary ✅ hardened

- ✅ compaction 子系统已实现。
- ✅ `PikoHost.compact()` / `maybeCompact()` 已接 settings,返回 `CompactResult` 含错误详情。
- ✅ `compact()` 失败时抛出异常，TUI `/compact` 可正确显示错误。
- ✅ `runCompact()` / `runMaybeCompact()` 返回结构化结果，不再静默吞错。
- ✅ 19 个 compaction 单元测试覆盖 token 估计、阈值判断、cut point、prep 流程。
- ✅ branch navigation 已有 `generateAutoBranchSummary()` + `branchWithSummary()` 路径。

### OAuth ✅ upgraded

- ✅ RFC 8628 标准 device-code polling（`pollOAuthDeviceCodeFlow`）。
- ✅ AbortSignal 支持，用户按 Escape 可取消。
- ✅ 正确 slow_down 处理（RFC 8628 §3.5：每次 +5s）。
- ✅ WSL/VM 时钟漂移错误提示。
- ✅ 取消时显示 "Login cancelled" 而非 "error"。
- ✅ TUI OAuth dialog 已接入 AbortController。

---

## P4 - TUI Polish 🟡 partial

### 当前状态

多数 UI 功能已经存在:

- `/model` / `/models` / `/thinking`
- `/settings`
- `/login` / `/logout`
- `/resume` / `/sessions` / `/tree` / `/fork` / `/clone` / `/new`
- `/skill` / `/template`
- `/compact` / `/export` / `/reload`
- image paste / file argument processing

### 本次推进

- ✅ Queue visibility - `QueueUpdateEvent` 增加 `steerPreview` / `followUpPreview` / `nextTurnPreview`(truncated message text)
- ✅ TUI `doSubmit` 通过 `onLifecycleEvent` 消费 `queue_update`,在 status line 显示 pending steering/followUp(含 message preview)
- ✅ `host-tui` 已导入 `HostLifecycleEvent`,lifecycle wiring 已接通

### 剩余风险

- 🟡 header/footer 的 model、thinking、session 信息应持续来自同一 runtime source。
- 🟡 `/login` 后 model registry/provider config refresh 需要更多 smoke。
- 🟡 `/settings` 后 theme/thinking/model scope/compaction/retry 的运行时效果需要更完整验证。
- 🟡 key hints 仍较粗粒度。
- 🟡 OAuth UI 尚未达到 pi 等价。

---

## Extension / Hook Surface 🔶 deferred

这块先暂缓,不作为当前"和 pi 行为一致"的同等优先级事项。pi 的 runtime 是 monolithic agent/session/UI 结构下形成的 API;piko 的 Host + stateless Engine 架构不同,extension surface 后续应重新设计。

当前记录这个章节只为防止后续误把 extension parity 放回主线优先级:

- 不要求 piko 复刻 pi extension runtime API。
- 不把 extension runtime 作为当前 pi 行为一致性验收项。
- piko 扩展性等 core coding agent 功能完整、Host/Engine 边界稳定后,再做 piko-native 设计。

后续重新启动扩展性设计时,可以再评估这些能力是否需要进入 piko-native extension surface:

- provider request/payload hooks
- context transform hook
- tool_call / tool_result interception
- resources discovery
- extension-provided tools 接入 agent tool set
- hook error normalization

**当前状态:**

- ✅ `host-tui` 有轻量 `ExtensionHost`,支持注册 TUI command、UI helper、event handler。
- 🟡 `registerTool()` 目前只进入 TUI extension host 的数组,尚未接入 `engine-native` tool registry / tool definitions。
- 🔶 host-runtime hook contract / extension bridge 暂缓,不进入当前优先级列表。

---

## 剩余非扩展缺口

当前剩余项主要是体验和 hardening：

- ✅ Compaction / branch summary:错误可见性、设置覆盖、边界行为已完成测试覆盖（19 tests）。
- ✅ OAuth:升级到 RFC 8628 标准，支持 AbortSignal 取消。
- 🟡 TUI settings/login consistency:`/login` 后 provider/model refresh、`/settings` 后 runtime state 刷新仍需要端到端 smoke。
- 🟡 Queue polish:queued skill/template expansion 与普通 prompt 路径还需要进一步收敛。
- 🟡 package-installed skills:低优先级,未纳入当前主线完成标准。

---

## 当前质量 Gate

最近一次复核结果:

- ✅ `npm run check` 通过。
- ✅ `npm test` 通过：15 test files / 67 tests（无需手动设置 HOME）。

---

## 下一步优先级

1. ~~扩展 host lifecycle~~ ✅ Phase 1 完成
2. ~~重构 turn state~~ ✅ Phase 2 完成
3. ~~接通 active tools~~ ✅ Phase 3 完成(engine-native 已修复 input.tools 覆盖,Host 已改为显式 ActiveToolsState)
4. ~~改测试环境默认 HOME/session dir~~ ✅ Phase 0 完成(root vitest.config.ts + setupFiles)
5. ~~TUI 一致性和体验收敛~~ ✅ Phase 4 主要项完成
6. 扩展性暂缓:等 core coding agent 功能完整后,再设计 piko-native extension surface。
