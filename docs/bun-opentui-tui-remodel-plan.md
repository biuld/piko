# Bun + OpenTUI TUI Remodel Plan

## Goal

在独立分支上把 piko 切到 Bun runtime，并借这个机会移除 `@earendil-works/pi-tui`，对 `host-tui` 做一次状态驱动的重新建模，然后引入 OpenTUI + SolidJS 作为新的终端 UI 渲染层。

这不是一次简单的 TUI 组件库替换。目标是把当前命令式 TUI 改成清晰的 Host UI model，并同步重做终端布局模型。

这里的“重新建模”包含两层，而且两者必须一起设计：

- 架构模型：state、action、reducer、side effects、renderer 边界。
- 布局模型：信息层级、屏幕分区、scroll/focus 规则、overlay 形态、窄屏降级、实时运行状态的摆放方式。

布局不是 renderer 的末端细节。布局决定 state 需要表达哪些概念，例如 active panel、scroll anchor、compact mode、selected row、transient command state、tool block collapse state、bottom bar density；反过来，state 的粒度也会限制布局能不能稳定演进。

目标是：

- Host runtime 继续负责 session、scheduler、settings、auth、models、skills、compaction。
- Engine 继续保持 stateless。
- TUI 层只消费 UI state，发出 UI action。
- OpenTUI + SolidJS 只作为 renderer，不反向污染 Host/Engine 边界。

## Branch Strategy

在独立分支上直接切换，不在主线渐进混用两套 TUI：

```bash
git checkout -b bun-opentui-tui-remodel
```

分支目标：

- 默认开发命令切到 Bun。
- 移除 `@earendil-works/pi-tui` runtime 依赖。
- 保留 `@earendil-works/pi-ai`，除非实测发现 Bun blocker。
- 新建 OpenTUI + SolidJS TUI 实现。
- 通过状态模型和布局模型隔离 renderer，避免未来再次重写 TUI 时触碰 Host runtime。

合并主线前必须满足：

- `bun run check` 通过。
- `bun run test` 通过。
- `piko --help`、`piko --list-models` 在 Bun 下通过。
- TUI 手动 smoke 通过：输入、流式输出、tool call、abort、resize、model selector、resume selector、settings selector、login flow。

## Current Constraints

当前 piko 是 npm workspaces + Node CLI：

- 根脚本使用 `npm run ...` 和 `node packages/cli/bin/piko`。
- CLI shebang 是 `#!/usr/bin/env node`。
- `host-tui` 依赖 `@earendil-works/pi-tui`。
- `host-tui` 的 UI state、组件实例、stream event mutation 混在一起。

已经验证的 Bun 兼容性：

- `bun run check` 通过。
- `bun run test` 通过。
- `bun packages/cli/bin/piko --help` 通过。
- `bun packages/cli/bin/piko --list-models` 通过。
- `@earendil-works/pi-ai` 的 Faux provider stream 可在 Bun 下运行。

## Dependency Decision

### Remove

- `@earendil-works/pi-tui`

原因：

- 它是当前阻碍 Bun + OpenTUI 的核心 TUI runtime。
- 它的组件模型和 `host-tui` 当前命令式结构绑定较深。
- 继续兼容它会拖慢 TUI 重新建模。

### Keep

- `@earendil-works/pi-ai`

原因：

- piko 的 engine/provider adapter 已经围绕 `pi-ai` 类型和 stream API 工作。
- `pi-ai` 已有 Bun 兼容判断，例如 `process.versions?.bun`。
- OpenAI、Anthropic、Google、Mistral 的主路径主要走 Web Fetch/Web Streams 或 SDK。
- OAuth 中使用的 `node:http`、`node:crypto` 在 Bun 下可用。

### Watch

- OpenAI Codex WebSocket transport
- Anthropic OAuth browser callback
- Bedrock AWS profile/proxy/HTTP1 fallback
- Google Vertex ADC
- Proxy 环境下的 `proxy-from-env` 动态 import

`pi-ai` 的 `openai-codex-responses` 在 Bun + proxy env 时会动态 import `proxy-from-env`，但当前 `@earendil-works/pi-ai` package 没声明这个依赖。默认无 proxy 不触发；如果要支持代理环境，需要 patch。

## Bun Migration

第一阶段直接把开发和运行入口切到 Bun。

推荐脚本形态：

```json
{
  "scripts": {
    "start": "bun run build && bun packages/cli/bin/piko",
    "piko": "bun packages/cli/bin/piko",
    "check": "biome check && tsc -b --pretty false",
    "fmt": "biome check --fix",
    "clean": "rm -rf packages/*/dist",
    "build": "bun run fmt && tsc -b",
    "test": "vitest run",
    "check:all": "bun run check && bun run test"
  }
}
```

CLI shebang 改成：

```bash
#!/usr/bin/env bun
```

保留 TypeScript project references 和 `tsc -b`。不要在这一阶段引入 bundler，避免把 runtime 切换和 packaging 切换混在一起。

## TUI Remodel

当前 `host-tui` 的核心问题不是组件库，而是状态所有权和布局所有权都不清晰：

- `BaseApp` 持有组件实例、状态、extension runtime、overlay、editor replacement。
- stream event 直接修改 `chatView/statusLine/spinner/footer/tui`。
- `ChatView` 同时管理消息数据、tool block 生命周期和组件树重建。
- overlays 各自处理 focus、filter、render、select/cancel。
- 布局没有一等模型，chat/status/editor/bottom bar/overlay 的关系散落在组件实现里。
- 长消息、tool block、overlay、resize、窄屏降级之间缺少统一规则。

新的模型应该拆成五层。这里的 UI state 必须和 layout 一起设计，不能先抽一个“纯 UI state”，再让 layout 自己解释它。

### 1. UI State

新增 `packages/host-tui/src/state/`：

- `state.ts`
- `events.ts`
- `actions.ts`
- `reducer.ts`
- `selectors.ts`

核心 state 示例：

```ts
export interface TuiState {
  session: TuiSessionState;
  model: TuiModelState;
  transcript: TuiMessageViewModel[];
  stream: TuiStreamState;
  usage: TuiUsageState;
  layout: TuiLayoutState;
  input: TuiInputState;
  overlay: TuiOverlayState | null;
  extensions: TuiExtensionSlots;
}
```

`TuiLayoutState` 不应该只是 terminal width/height。它要表达布局决策：

```ts
export interface TuiLayoutState {
  viewport: { width: number; height: number };
  mode: "regular" | "compact" | "minimal";
  activeRegion: "chat" | "editor" | "overlay";
  bottomBar: {
    density: "full" | "compact" | "minimal";
    visibleFields: Array<"model" | "session" | "branch" | "tokens" | "cost" | "cwd" | "mode" | "hints">;
  };
  chat: {
    scrollAnchor: "bottom" | "selection" | "manual";
    selectedMessageId?: string;
    collapsedToolCallIds: Set<string>;
  };
  overlay?: {
    kind: TuiOverlayKind;
    placement: "modal" | "drawer" | "inline";
  };
}
```

这让布局策略可测试，也让 OpenTUI/Solid renderer 不需要自己推断“当前应该怎么排版”。

UI state 分三类：

- Domain state：来自 Host/Engine 的事实，例如 transcript、current model、session、usage、running status。
- View state：用户当前怎么看这些事实，例如 selected message、expanded tool call、active overlay、input draft、model selector query。
- Layout state：这些 view state 在当前终端尺寸下如何摆放，例如 layout mode、bottom bar density、visible fields、overlay placement、scroll anchor。

三者的关系是单向派生为主：

```text
Domain state + View state + viewport
  -> layout policies/selectors
  -> Layout state + render view models
  -> OpenTUI/Solid renderer
```

原则：

- Domain state 不应该包含 width/height、density、placement。
- Layout state 不应该重新保存 transcript 或 provider 事实。
- View state 可以被 layout 影响，例如窄屏下 selected message 仍保留，但 display fields 变少。
- Renderer local state 只保留焦点实现细节和瞬时输入法/光标状态，不保存可恢复的 UI 决策。
- 如果某个状态会影响快捷键、滚动、overlay、tool collapse 或跨 render 保持，就不要放在组件私有 state。

### 2. Layout Model

布局需要作为产品模型处理，而不是边写组件边决定。

主屏推荐分区：

```text
┌──────────────────────────────────────────────┐
│ Chat timeline                                │
│ - user/assistant messages                    │
│ - tool calls                                 │
│ - branch / compaction markers                │
│ - running turn progress near relevant block  │
├──────────────────────────────────────────────┤
│ Status: queue / running tool / warnings      │
├──────────────────────────────────────────────┤
│ Editor                                       │
├──────────────────────────────────────────────┤
│ Bottom bar: model / session / cwd / hints    │
└──────────────────────────────────────────────┘
```

布局原则：

- Chat 是主区域，应该优先获得高度。
- Editor 高度随内容增长但必须有上限，不能挤掉 chat。
- Running/tool 状态应该靠近 chat timeline，同时 status line 保留全局摘要。
- 不设常驻 header。顶部空间留给 chat timeline。
- 原 header/footer 合并成 sticky bottom bar，始终贴底显示。
- Bottom bar 只放扫描信息，不承载长文本。
- 当前全局状态进 bottom bar，当前运行态进 status line，历史上下文进 chat timeline marker。
- Overlay 分 modal、drawer、inline 三类，不同场景选不同形态。
- 窄屏时优先隐藏辅助信息，而不是压缩核心文本到不可读。
- Tool call 默认应该是稳定高度的摘要块，可展开查看详情。
- Resume/tree selector 需要专门的树布局模型，不应复用普通列表的状态。

布局和架构的联动点：

- 如果 tool block 可折叠，collapse state 必须进 `TuiLayoutState` 或 message view model。
- 如果 chat 支持手动滚动，stream delta 到来时不能无条件 scroll bottom。
- 如果 overlay 是 drawer，底层 chat/editor 的 focus 和尺寸仍需可预测。
- 如果 model selector 有 search/scope/selection，state 需要表达 query、scope、selected id，而不是让组件私有保存。
- 如果 compact mode 会隐藏 token/cost/provider 细节，bottom bar selector 要提供 display view model，不能在 JSX 里临时拼接。
- 如果 bottom bar 同时显示上下文和快捷键，state 需要表达 density 和 visible fields，不能让组件用 width 临时决定业务优先级。

### 3. UI Events

所有外部变化先进入 UI event：

- `user_input_changed`
- `user_submitted`
- `stream_started`
- `assistant_delta`
- `thinking_delta`
- `tool_call_started`
- `tool_call_finished`
- `turn_finished`
- `turn_failed`
- `overlay_opened`
- `overlay_closed`
- `layout_resized`
- `region_focused`
- `chat_scrolled`
- `tool_block_toggled`
- `model_changed`
- `session_resumed`
- `session_forked`

stream handler 只做 event dispatch，不直接操作 UI 组件或布局组件。

### 4. UI Actions

actions 负责副作用：

- submit prompt
- abort current run
- resume session
- fork session
- switch model
- login provider
- invoke skill/template

Action 可以调用 `PikoHost`，但 renderer 不直接调用 `PikoHost`。

### 5. Renderer

OpenTUI + SolidJS renderer 只做：

- 订阅 state。
- 渲染 chat、status、editor、bottom bar、overlay。
- 把 keyboard/input/select 操作转成 action。
- 按 `TuiLayoutState` 渲染布局，不在组件内部重新计算产品级布局规则。

renderer 不持有业务状态。局部 UI 状态只允许用于临时输入焦点、hover/selection 等不需要 session 持久化的东西。

## OpenTUI + SolidJS Plan

新增 OpenTUI renderer 包内实现，优先保持包名不变：

```text
packages/host-tui/
  src/
    app/
      index.ts
      runtime.ts
    state/
      state.ts
      reducer.ts
      actions.ts
      selectors.ts
    layout/
      model.ts
      measure.ts
      policies.ts
    renderer/
      opentui/
        App.tsx
        Chat.tsx
        Editor.tsx
        BottomBar.tsx
        StatusLine.tsx
        overlays/
```

优先实现最小闭环：

1. App shell
2. editor input
3. submit prompt
4. streaming assistant text
5. tool call start/end
6. status/bottom bar
7. abort
8. resize and compact layout mode

然后迁移 overlays：

1. model selector
2. thinking selector
3. settings selector
4. resume selector
5. fork/tree selector
6. login/oauth dialog
7. rename prompt

不要一开始迁移 extension API。先让核心 TUI 稳定，再设计 extension slot adapter。

## Testing Strategy

自动测试：

- reducer unit tests
- selectors unit tests
- layout policy tests
- command/action tests with FauxProvider
- transcript to view-model tests

手动 smoke：

- launch TUI
- submit normal prompt
- stream text
- run tool call
- approve/reject tool call
- Ctrl+C abort
- terminal resize
- compact/narrow layout
- scroll behavior during streaming
- tool block collapse/expand
- model selector search and select
- resume selector search and select
- settings selector edit
- login flow cancel/success path

如果 OpenTUI 提供 test renderer，优先用它做 snapshot 或 structural tests。不要依赖 ANSI snapshot 作为唯一验证，因为 ANSI 输出对终端能力和 width 很敏感。

## pi-ai Fallback

默认继续 npm 依赖 `@earendil-works/pi-ai`。

如果遇到 Bun blocker，兜底方案是在 piko 仓库内维护一份可 patch 的 `pi-ai`：

```text
vendor/pi-ai/
packages/pi-ai/
```

或拆成 workspace 包：

```text
packages/pi-ai/
```

推荐优先级：

1. 使用 npm `@earendil-works/pi-ai`。
2. 用 `patch-package` 或 Bun patch 解决小问题。
3. 如果 patch 需要长期维护，再 vendor 到 `packages/pi-ai`。

需要 vendor 的触发条件：

- Bun runtime 下 provider 主路径失败，且 upstream 短期无法修。
- 缺依赖或动态 import 问题反复出现。
- OAuth / proxy / WebSocket 需要 piko 专属行为。
- 类型或 stream event 需要与 piko engine protocol 更紧密对齐。

vendor 后的规则：

- 保留 upstream commit/version 标记。
- 每个 patch 都写明原因。
- 不做无关格式化。
- 每次 upstream 升级用小步 rebase。
- 对 piko 使用到的 provider 增加 Bun smoke tests。

第一批可能需要 patch 的点：

- `proxy-from-env` 缺依赖。
- OpenAI Codex WebSocket proxy handling。
- Bun compiled binary env fallback 的平台覆盖。
- Bedrock request handler 在 Bun 下的代理行为。

## Phases

### Phase 0: Branch and Bun Baseline

- 新建 `bun-opentui-tui-remodel` 分支。
- 增加 `bun.lock`。
- 脚本切到 Bun。
- CLI shebang 切到 Bun。
- 保持现有 TUI 不动，先确认 baseline。

验收：

- `bun install`
- `bun run check`
- `bun run test`
- `bun run piko --help`
- `bun run piko --list-models`

### Phase 1: State and Layout Model Extraction

- 新建 TUI state/reducer/actions/selectors/layout policies。
- 把 transcript/tool call/status/usage/input/overlay 建模。
- 把 viewport、layout mode、active region、bottom bar density、visible fields、scroll anchor、tool collapse、overlay placement 建模。
- 旧 `pi-tui` renderer 临时消费新 state，验证模型正确。

验收：

- 现有 TUI 仍可运行。
- reducer/selectors 有测试。
- layout policies 有测试。
- stream handler 不再直接修改 chat view 数据结构或布局状态。

### Phase 2: OpenTUI Minimal Renderer

- 引入 OpenTUI + SolidJS。
- 实现最小 TUI 闭环。
- 暂时禁用或 stub 非核心 overlays。
- 实现 regular/compact/minimal 三档布局。

验收：

- TUI 可启动。
- 可输入并提交。
- assistant delta 正常显示。
- tool call start/end 正常显示。
- abort 正常。
- resize 后布局稳定。

### Phase 3: Overlay Migration

- 迁移 model/thinking/settings/resume/fork/login overlays。
- 实现统一 focus 和 selection model。
- 恢复主要 keyboard shortcuts。

验收：

- overlay smoke 全通过。
- resize 不破布局。
- 长 transcript 可滚动。

### Phase 4: Remove pi-tui

- 删除 `@earendil-works/pi-tui` 依赖。
- 删除旧 renderer 和旧组件。
- 清理旧命令式 ChatView/BaseApp 结构。

验收：

- `rg "@earendil-works/pi-tui"` 无 runtime import。
- `bun run check` 通过。
- `bun run test` 通过。

### Phase 5: pi-ai Hardening

- 为 piko 实际使用的 provider 加 Bun smoke。
- 验证 OAuth、proxy、WebSocket、Bedrock/Vertex 关键路径。
- 需要时 patch 或 vendor `pi-ai`。

验收：

- 常用 provider 在 Bun 下可用。
- fallback policy 写入 README 或 docs。
- 如果 vendor，必须有 upstream version note。

## Non-goals

- 不在这次迁移中重写 engine。
- 不改变 session JSONL 格式。
- 不改变 host/engine protocol。
- 不引入 bundler 作为运行前提。
- 不把 OpenTUI 细节泄漏到 host-runtime。

## Final Shape

目标完成后：

- piko 默认 Bun runtime。
- `host-tui` 是状态驱动的 OpenTUI + SolidJS renderer。
- `host-runtime` 和 `engine-*` 不知道 OpenTUI 存在。
- `pi-tui` 完全移除。
- `pi-ai` 继续作为 provider 层依赖；必要时 piko 有明确 patch/vendor 兜底。
