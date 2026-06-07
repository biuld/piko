# host-tui runtime boundary redesign

## 背景

当前 `host-tui` 已经从早期的单文件 TUI 走向了多子系统结构,包括:

- `runtime/TuiController`
- `state/TuiState` + reducers
- `timeline`
- `surfaces`
- `focus`
- `commands`
- `autocomplete`
- `renderer/opentui`

但最近的 slash autocomplete、发送内容闪烁、timeline 显示异常暴露了一个核心问题:

**交互态、布局态、渲染态、持久业务态没有严格分层。**

表现为:

- `/` 触发 autocomplete 时会影响全局 store 或 timeline 布局。
- autocomplete 显示/隐藏会挤压或覆盖 timeline scrollbox。
- 发送内容时多个全局事件连续 dispatch,根 App 和 timeline 被重复刷新。
- `turn_finished` 需要同时 reconcile transcript 和 timeline items,一旦 ID 复用策略错误,timeline 会显示异常。
- `SurfaceManager` 被用来承载局部输入提示,但 autocomplete 本质上是 Editor 的附属交互,不应和全局 selector/modal surface 同级。

这不是单个组件实现问题,而是 TUI runtime boundary 需要重新划分。

## 设计目标

1. 普通输入、slash 输入、file autocomplete 输入都不能触发 timeline resize。
2. Editor 的临时状态不能进入全局 `TuiState`。
3. Timeline 只响应 timeline 相关事件,不被 Editor/autocomplete/surface 显隐影响。
4. SurfaceManager 只管理跨区域 UI,不管理输入框内部提示。
5. Host/session/stream/transcript 作为业务状态,Timeline 作为 view state,两者必须有明确同步协议。
6. 每个子系统的 owner 明确,避免 `TuiController` 变成所有交互的集中杂物箱。

## 当前架构问题

### 1. `TuiState` 承载了过多 UI 细节

当前全局 state 既包含 durable 状态:

- session
- stream
- transcript
- timeline
- model
- usage
- notifications

也曾承载高频临时 UI 状态:

- input text
- autocomplete active
- autocomplete selected index
- autocomplete items/prefix/provider

高频状态进入全局 store 后,任何字符输入都可能导致:

- `App.tsx` 重新计算 render plan
- `SlotRenderer` 重新给 `TimelineView` 传 props
- `TimelineView` scrollbox 重新参与布局或绘制
- terminal scrollbar 闪烁

目标:

`TuiState` 只保存 durable view state,不保存 keystroke-level ephemeral state。

### 2. Autocomplete 和 Surface 边界错误

文档中曾把 slash autocomplete 设计为 anchored surface,但实际运行中它更接近 Editor 内部附属控件。

如果 autocomplete 通过 SurfaceManager open/close:

- surface list 变化会触发 render plan 变化
- Portal/overlay 可能和 scrollbox 重叠绘制
- anchored surface 的显示/隐藏会影响根 App 的渲染树

如果 autocomplete inline 渲染在 Editor:

- 出现/消失会改变 Editor 高度
- timeline scrollbox 高度随之变化

如果 autocomplete absolute overlay 覆盖 timeline:

- 不改变布局,但会与 scrollbox 发生同一区域重绘
- terminal 仍可能表现为滚动条抖动

目标:

Autocomplete 不走全局 surface manager。它应是 Editor 子系统内部的固定 viewport/popup lane,并且其布局空间在 Editor 初始化时稳定确定。

### 3. Timeline state 和 transcript reconcile 耦合脆弱

当前 App 渲染 `state.timeline.items`,不再每次从 `transcript` 现算。这是正确方向,但带来一个要求:

所有会改变 transcript 的 reducer 必须同步维护 timeline items。

风险点:

- `user_submitted` 添加 user transcript 时必须添加 timeline item。
- `assistant_delta` 创建或更新 streaming item。
- `tool_call_started/ended` 必须按 `toolCallId` 更新同一个 timeline item。
- `turn_finished` 必须 reconcile canonical engine transcript 和 streaming transcript,且保持 stable IDs。
- `turn_failed` 不能只写 transcript,也要写 timeline item。
- `session_resumed` 可以从 transcript 初始化 timeline items,但后续 live path 不能反复 rebuild 全量 items。

目标:

Timeline 需要独立 reducer/controller,通过明确的事件协议增量维护 items。

## 新边界设计

### Layer 1: Host runtime state

Owner: `host-runtime`

职责:

- session lifecycle
- model config
- auth/settings
- prompt/skills/context
- scheduler/stream lifecycle
- canonical transcript persistence

不负责:

- terminal layout
- scroll position
- autocomplete selected index
- keybinding UI

输出给 TUI:

```ts
type HostViewEvent =
  | { type: "turn_started"; userText: string }
  | { type: "assistant_delta"; delta: string }
  | { type: "thinking_delta"; delta: string }
  | { type: "tool_call_started"; id: string; name: string; args: unknown }
  | { type: "tool_call_ended"; id: string; result: unknown; isError: boolean }
  | { type: "turn_finished"; transcript: Message[]; status: string }
  | { type: "turn_failed"; error: string }
  | { type: "queue_update"; steerCount: number; followUpCount: number };
```

### Layer 2: TUI durable state

Owner: `packages/host-tui/src/state`

应该保存:

- session summary
- stream status
- model/usage/status line data
- notifications
- active global surfaces
- timeline state
- focus owner for global surfaces

不应该保存:

- editor draft
- autocomplete query
- autocomplete result list
- autocomplete selected index
- local filter text inside selector unless selector is a global surface controller

建议拆分:

```ts
interface TuiState {
  session: TuiSessionState;
  stream: TuiStreamState;
  model: TuiModelState;
  usage: TuiUsageState;
  notifications: TuiNotification[];
  timeline: TuiTimelineState;
  surfaces: TuiSurfaceState[];
  focus: TuiFocusState;
}
```

删除或避免扩展:

```ts
autocomplete?: TuiAutocompleteState;
input: { text: string };
```

### Layer 3: Timeline subsystem

Owner: `packages/host-tui/src/timeline`

职责:

- stable timeline item IDs
- stream item append/update/finalize
- tool item append/update
- transcript reconcile
- scroll anchor state
- pending new output count
- expansion/collapse state

Timeline reducer API:

```ts
type TimelineEvent =
  | { type: "timeline/user_appended"; text: string; messageId: string }
  | { type: "timeline/assistant_delta"; messageId: string; fullText: string }
  | { type: "timeline/tool_started"; toolCallId: string; name: string; args: unknown }
  | { type: "timeline/tool_ended"; toolCallId: string; result: unknown; isError: boolean }
  | { type: "timeline/reconciled"; transcript: TuiMessageViewModel[] }
  | { type: "timeline/scroll_anchor_changed"; anchor: "bottom" | "manual" }
  | { type: "timeline/item_toggled"; itemId: string };
```

规则:

- `TimelineView` 只订阅 `timeline`.
- `Editor` 不直接写 `timeline`.
- `ActionService` 把 HostViewEvent 转成 state event,由 reducer 更新 timeline。
- `turn_finished` 不能全量重建所有 item 对象;应 preserve unchanged object identity。
- 每个 timeline item 的 ID 必须稳定:
  - message: `msg:${messageId}`
  - tool: `tool:${toolCallId}`
  - branch summary: `branch:${entryId}`
  - compaction summary: `compaction:${summaryId}`

### Layer 4: Editor subsystem

Owner: `packages/host-tui/src/renderer/opentui/editor` 或 `packages/host-tui/src/editor`

职责:

- draft text
- cursor/composition
- submit intent
- local autocomplete controller
- local input key handling

状态:

```ts
interface EditorLocalState {
  draft: string;
  disabled: boolean;
  autocomplete: EditorAutocompleteState;
}
```

Editor 对外只发 high-level action:

```ts
type EditorAction =
  | { type: "submit_prompt"; text: string }
  | { type: "execute_command"; command: string; args?: string }
  | { type: "open_global_surface"; surface: "model" | "settings" | "resume" };
```

Editor 不应该:

- dispatch `user_input_changed`
- dispatch `autocomplete_active`
- 修改 `timeline`
- open/close autocomplete surface

### Layer 5: Autocomplete subsystem

Owner: Editor

职责:

- provider query
- async cancellation
- result list
- selected index
- accept/cancel
- render model

建议接口:

```ts
interface EditorAutocompleteController {
  state(): EditorAutocompleteState;
  query(input: string, cursor: number): void;
  move(delta: number): void;
  accept(): AutocompleteApplyResult | null;
  cancel(): void;
}

interface EditorAutocompleteState {
  visible: boolean;
  loading: boolean;
  query: string;
  providerId?: string;
  prefix: string;
  items: AutocompleteItem[];
  selectedIndex: number;
}
```

渲染规则:

- Autocomplete lane 是 Editor 的一部分。
- lane 高度固定,不随 items 数量变化。
- lane 不覆盖 timeline scrollbox。
- lane 不进入 global surface render plan。
- `/` 出现时只更新 Editor local signal,不触发 root store dispatch。

布局建议:

```txt
timeline scrollbox  flexGrow=1
status line         fixed
editor shell        fixed
  autocomplete lane fixed height, hidden content when inactive
  input row         fixed height
bottom bar          fixed
```

如果要完全不常驻占空间,需要 renderer 级 overlay host 支持“不参与 root layout 且不覆盖 scrollbox 的 reserved overlay plane”。在当前 OpenTUI 行为未验证前,固定 lane 更稳。

### Layer 6: Surface subsystem

Owner: `packages/host-tui/src/surfaces`

只管理跨区域 UI:

- model selector
- thinking selector
- resume selector
- settings
- login
- notifications
- help/hotkeys
- fork/session tree
- confirm dialog

不管理:

- slash autocomplete
- file path autocomplete
- inline editor hint
- transient input composition UI

Surface state:

```ts
interface TuiSurfaceState {
  id: string;
  role: SurfaceRole;
  mount: SurfaceMount;
  blocking: boolean;
  parentId?: string;
  targetSlot?: SurfaceSlot;
  zIndex: number;
  data?: unknown;
}
```

Surface render plan 只影响 base slots:

- timeline
- status
- editor
- bottom-bar

规则:

- Opening a global surface may affect layout.
- Typing inside Editor must never open/close a global surface.
- Surface focus is for global surfaces only.
- Editor local autocomplete uses Editor key handler, not FocusManager global interceptor.

### Layer 7: Focus and keymap

Focus should distinguish:

1. Global focus owner:
   - editor
   - timeline
   - global surface
2. Local widget focus:
   - editor autocomplete
   - selector list row
   - settings child list

Global FocusManager handles:

- Esc closing global surface
- PageUp/PageDown timeline scroll when editor allows it
- global command shortcuts

Editor handles locally:

- autocomplete Up/Down
- autocomplete Tab
- autocomplete Esc
- Enter behavior when selected slash command exists

Key priority:

```txt
local focused widget
  -> global focused surface/owner
  -> global app shortcuts
  -> input default behavior
```

## Event flow

### Normal text input

```txt
input onInput
  -> Editor draft signal
  -> no TuiState dispatch
  -> no TimelineView prop change
```

### Slash autocomplete

```txt
input "/"
  -> Editor draft signal
  -> EditorAutocompleteController.query("/")
  -> local suggestions signal
  -> AutocompleteLane rerenders only
  -> no root store dispatch
  -> no surface open/close
  -> no timeline resize
```

### Submit prompt

```txt
Enter
  -> Editor emits submit_prompt(text)
  -> ActionService batchDispatch(user_submitted + stream_started)
  -> timeline appends user item
  -> stream events incrementally update assistant/tool items
  -> turn_finished reconciles IDs without replacing unchanged items
```

### Execute slash command

```txt
Enter while selected slash item
  -> Editor local autocomplete returns command
  -> CommandRegistry executes command
  -> command may open global surface if needed
```

Examples:

- `/model` opens model selector surface.
- `/resume` opens resume selector surface.
- `/help` opens help surface.
- plain `/unknown` sends notification.

## Migration plan

### Phase 1: Stabilize current behavior

Tasks:

- Remove all `user_input_changed` dispatches from per-key input.
- Remove `autocomplete_active`, `autocomplete_navigate`, `autocomplete_accept` from `TuiState`.
- Keep autocomplete selected index local to Editor.
- Keep autocomplete results local to Editor.
- Ensure slash autocomplete does not call `SurfaceManager.openSurface()`.
- Keep autocomplete lane fixed height.
- Batch submit startup events.
- Fix `turn_finished` transcript/timeline reconcile.

Acceptance:

- Typing `abc` produces no global dispatch.
- Typing `/` produces no global dispatch.
- Up/Down/Tab/Esc in autocomplete produces no global dispatch.
- Sending prompt dispatches one batched update before stream starts.
- Timeline does not lose or duplicate user/tool/assistant items after `turn_finished`.

### Phase 2: Extract EditorAutocompleteController

Create:

```txt
packages/host-tui/src/editor/
  editor-autocomplete-controller.ts
  editor-autocomplete-state.ts
  editor-actions.ts
```

Move out of `Editor.tsx`:

- async provider query
- cancellation
- selected index
- accept/apply
- visible/loading state

`Editor.tsx` becomes renderer wiring only.

Acceptance:

- Controller has unit tests without OpenTUI.
- Slash provider and file provider are tested through combined provider.
- Race condition test: slower stale query result cannot replace newer query result.

### Phase 3: Extract TimelineController/reducer

Create:

```txt
packages/host-tui/src/timeline/
  timeline-reducer.ts
  transcript-reconcile.ts
  timeline-events.ts
```

Move from generic state reducers:

- user append
- assistant streaming update
- tool start/end update
- turn finished reconcile
- failure item append
- scroll anchor updates

Acceptance:

- Unit tests cover multi-turn canonical transcript.
- Tool call start/end/result maps to one `tool:${toolCallId}` item.
- `turn_finished` preserves object identity for unchanged items.
- Error turn appears in timeline.

### Phase 4: SurfaceManager scope cleanup

Tasks:

- Remove autocomplete role from global surface manager or mark it deprecated.
- Keep `autocomplete` docs under Editor subsystem, not global surface docs.
- SurfaceManager only owns global/cross-slot UI.
- Surface render plan no longer has special autocomplete behavior.

Acceptance:

- `/model`, `/settings`, `/resume`, `/notifications` still open surfaces.
- Typing `/` does not mutate `state.surfaces`.
- Surface open/close does not affect Editor local autocomplete state except losing focus when blocking.

### Phase 5: Renderer instrumentation

Add dev-only tracing:

```ts
if (process.env.PIKO_TUI_TRACE) {
  console.error("[tui dispatch]", event.type);
}
```

Trace:

- global dispatch count per second
- render plan changes
- surface open/close
- timeline item count and changed IDs
- scrollbox viewport height changes

Acceptance:

- `abc` input: no global dispatch.
- `/` input: no global dispatch.
- autocomplete navigation: no global dispatch.
- submit: expected batch, then stream events.
- slash autocomplete does not change scrollbox viewport height.

## Target file layout

```txt
packages/host-tui/src/
  app/
  runtime/
    tui-controller.ts          # global orchestration only
  state/
    state.ts                   # durable TUI state
    reducers/
  editor/
    editor-actions.ts
    editor-autocomplete-controller.ts
    editor-autocomplete-state.ts
  autocomplete/
    provider.ts
    slash-provider.ts
    file-provider.ts
    combined-provider.ts
  timeline/
    timeline-events.ts
    timeline-reducer.ts
    timeline-builder.ts
    transcript-reconcile.ts
    scroll-controller.ts
  surfaces/
    surface-manager.ts
    surface-resolver.ts
    render-plan.ts
  focus/
  keymap/
  renderer/opentui/
    App.tsx
    SlotRenderer.tsx
    editor/
      Editor.tsx
      AutocompleteLane.tsx
    timeline/
      TimelineView.tsx
```

## Test plan

### Unit tests

- `editor-autocomplete-controller.test.ts`
  - slash query
  - file query
  - selected index clamp
  - accept completion
  - async stale result ignored

- `timeline-reducer.test.ts`
  - user append
  - assistant streaming append/update/finalize
  - tool start/end stable ID
  - turn_finished multi-turn reconcile
  - turn_failed appends visible item

- `surface-manager.test.ts`
  - global surfaces open/close
  - parent close closes children
  - no autocomplete surface mutation

### Runtime smoke

Manual or automated OpenTUI smoke:

1. Start TUI.
2. Type `abc`.
3. Type `/`.
4. Navigate suggestions.
5. Execute `/model`, close selector.
6. Submit prompt.
7. Stream assistant text.
8. Run a prompt that triggers tool calls.

Assertions:

- Terminal scrollbar does not flicker on `abc`.
- Terminal scrollbar does not flicker on `/`.
- Timeline height stays stable during autocomplete.
- Timeline appends user message immediately after submit.
- Assistant streaming updates one item, not multiple items.
- Tool call start/end updates one item.
- After final result, timeline order remains correct.

## Non-goals

- Do not move session persistence into `host-tui`.
- Do not make Engine aware of TUI timeline items.
- Do not use SurfaceManager for every small UI affordance.
- Do not rebuild the whole transcript-to-timeline list every render.
- Do not introduce a new renderer before isolating state boundaries.

## Decision

Adopt this boundary model:

```txt
Host runtime: canonical session and stream events
TUI durable state: session/stream/model/usage/notifications/surfaces/timeline
Timeline: stable item state and scroll behavior
Editor: draft and local interaction
Autocomplete: Editor-owned local controller
SurfaceManager: global cross-slot UI only
Renderer: pure projection of state/controllers to OpenTUI nodes
```

This design directly targets the observed issues:

- Slash autocomplete no longer mutates global state.
- Autocomplete no longer opens/closes global surfaces.
- Timeline scrollbox no longer resizes or overdraws due to slash suggestions.
- Submit startup updates are batched.
- Transcript/timeline reconcile is isolated and testable.

