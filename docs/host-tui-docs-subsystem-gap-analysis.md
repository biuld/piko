# host-tui 文档子系统实现差距分析

日期:2026-06-04(生成) / 2026-06-04(审查更新)

范围:对照 `packages/host-tui/docs/*.md` 的目标设计,检查 `packages/host-tui/src` 当前实现是否已经完整覆盖各个 TUI UX runtime 子系统。

结论:7 大子系统(timeline、keymap、autocomplete、focus、surface render plan、commands、notifications)均已落地核心实现。剩余缺口集中在:config 文件加载、autocomplete 的 surface 化渲染、settings 级别的 selector/filter 行为细化,以及少数需要复杂 UI 的命令。

## 总体完成度

| 子系统 | 状态 | 说明 |
|---|---|---|
| package shape / runtime wiring | ✅ 完成 | `computeRenderPlan()` 驱动渲染,`SurfaceContentRegistry` 分发 surface,App.tsx 为纯 shell。 |
| commands | ✅ 基本完成 | 27 个命令中 22 个有实际实现(openSurface / host API / notify),剩余 5 个需复杂 UI。 |
| keymap | ✅ 基本完成 | 默认绑定全覆盖、`detectConflicts()` 已有、`formatHintLine()` 生成 hints;待接入文件加载。 |
| focus | ✅ 完成 | parent bubbling、`restoreTo` 存储、autocomplete/scroll 路由已移入 interceptor。 |
| surfaces | ✅ 完成 | SurfaceManager + resolver + occlusion + render-plan + SurfaceContentRegistry。 |
| notifications | ✅ 完成 | producers 已接入 stream/model/session 事件;history UI 可用。 |
| timeline | ✅ 完成 | stable IDs(`msg:${messageId}` / `tool:${toolCallId}`)、reducer 直接维护 items。 |
| selectors | ⚠️ 部分完成 | 共享 shell/list/controller 可用;filtering、列宽截断、动态 layout 待细化。 |
| autocomplete | ⚠️ 部分完成 | provider 架构(slash + file + combined)已落地;仍 inline 渲染在 Editor,未走 anchored surface。 |
| hints | ✅ 完成 | 所有硬编码 hint 已替换为 `KeymapManager.formatHintLine()`。 |
| layout / theme / renderer boundaries | ✅ 完成 | `computeRenderPlan` 替代手写 culling+slot 排序。 |

## 已经实现的关键点

- `TuiController` 初始化并串联 `KeymapManager`、`CommandRegistry`、`NotificationCenter`、`FocusManager`、`SurfaceManager`、`ScrollController`,并注册 builtin commands。
- Slash command registry 和 autocomplete provider 已能列出已注册命令,未知 slash command 会发送 error notification。
- `SurfaceManager` 支持 open/close、子 surface 关闭、z-index、occlusion 计算,`SurfaceHost` 按 mount 策略分发渲染。
- `FocusManager` 支持 active owner、stack、interceptors、global handler、push/pop/popTo。
- 通知中心支持 in-memory history、max 200、TTL、dedupe、mark read、clear、filter API,并同步到 store。
- `/notifications` 和 `/noti` 已注册并打开通知历史 surface。
- Timeline 已有类型、reducer、builder、scroll command、latest indicator 和按 item kind 渲染的组件。
- Bottom bar 已基本符合"状态数据为主",没有继续承担通用帮助文本。

## 主要遗漏和方案

### 1. Runtime 边界

> **状态:已收敛。** 2026-06-04 审查更新。

`App.tsx` 已通过 `computeRenderPlan(state())` 替代手写 culling + slot 排序(见 `packages/host-tui/src/surfaces/render-plan.ts`)。Timeline 从 `state().timeline.items` 读取(stable IDs),不再每次 render 重建。Surface content 分发已提取到 `packages/host-tui/src/renderer/opentui/surfaces/SurfaceContentRegistry.tsx`。`ReadOnlyListSurface` 也已迁移到同一文件。

**剩余问题:**

- **Slot renderer 仍在 App 内**:`renderSlot()` 函数直接在 App.tsx 中,负责 TimelineView/StatusLine/Editor/BottomBar 的 props 绑定和 scroll callbacks 注入。可以进一步抽到 `renderer/opentui/slots/` 或由 plan entry 携带 render 闭包。
- **Scroll callbacks 由 App 注入**:`TimelineView` 的 `onScrollStateChange` / `onScrollCommandDone` 还在 App shell 中 dispatch store events。可以改为 TimelineView 通过 controller 或 store 直接处理,不经过 App。

方案:把 `renderSlot` 提取为独立组件或由 render-plan 携带 render 函数,App 只做 `plan.map(entry => entry.render(ctx))`。

### 2. Commands 未完整实现 pi parity

> **状态:基本完成。** 2026-06-04 审查更新。

27 个命令中 22 个已有实际实现(openSurface 打开组件、host API 调用、或 notify 提示结果)。

**仍有 stub 的命令(2 个):**

| 命令 | 原因 |
|---|---|
| `/share` | host 无分享能力 |
| `/copy` | host 无复制能力 |

**已实现(25 个):** `/model` `/thinking` `/resume` `/settings` `/login` `/logout` `/new` `/compact` `/clone` `/fork` `/tree` `/name` `/notifications` `/hotkeys` `/changelog` `/session` `/help` `/quit` `/export` `/import` `/reload` `/scoped-models` model cycle Ctrl+P/N `app.tools.expand` Ctrl+O

方案:

1. `/fork` 实现消息选择器 surface,选完调用 `host.forkSession(entryId)`。
2. `app.tools.expand` 需要新增 store event 来 toggle `collapsedToolCallIds` 的全清/全设。
3. `/scoped-models` 可以复用 ModelSelector 但加 scope 过滤。

### 3. Keymap 配置层和绑定全集

> **状态：已完成。** 2026-06-04 审查更新。

- **默认绑定全集**：已补齐所有 `TuiKeybindingId` + `AppKeybindingId` 的默认映射，tui.* 绑定已标记 scope（editor/selector/autocomplete/timeline）。
- **文件加载**：`loadFromFiles(cwd)` 已接入，读取 `~/.piko/keybindings.json` + `.piko/keybindings.json`。
- **冲突检测**：`detectConflicts("global")` 启动时只报告 app 层冲突，不误报跨上下文复用。
- **hint 生成**：所有 selector/surface hints 由 `formatHintLine()` 生成。

**剩余问题：**

- 配置文件解析失败（无效 JSON、未知 binding id）静默忽略，无通知。
- override 不支持按 scope 覆盖（只支持按 binding id）。

### 4. Focus 嵌套和路由语义

> **状态:已完成。** 2026-06-04 审查更新。

- `handleKey()` 已支持从 deepest active owner 向 parent 递归冒泡(`packages/host-tui/src/focus/focus-manager.ts`)。
- `pushFocus()` 正确存储和使用 `restoreTo`,`closeSurface()` 按 restore target 恢复焦点。
- autocomplete navigation 和 timeline scroll (PageUp/PageDown/End) 已从 `TuiController.handleKey()` 移除,改为注册在 editor owner 的 interceptor 上(`packages/host-tui/src/runtime/tui-controller.ts`)。

**剩余问题:**

- focus state 变更未自动 dispatch 到 store。`FocusManager` 内部 state 更新后,store 中的 `state.focus` 可能成为过期快照。

方案:在 FocusManager 的关键方法(`pushFocus`/`popFocus`/`closeSurface`)中添加 callback/event,由 TuiController 同步 dispatch 到 store。

### 5. Surface resolver 和 culling

> **状态:已完成。** 2026-06-04 审查更新。

- `computeRenderPlan()`(`packages/host-tui/src/surfaces/render-plan.ts`)统一输出 base slots + surfaces + culled slots 的有序渲染列表。
- `side-drawer` 在 narrow 终端(< 80 列)时计入 `fullyCovers: ["timeline"]`。
- `replace-slot` 正确地替换对应 slot(culling 后不渲染原 slot,只在 replace-slot 区域显示 surface)。
- Surface content 通过 `SurfaceContentRegistry` 统一分发。

**剩余问题:**

- `insert-between` 的 `insertAfterSlot` 是固定映射(命令侧指定),未根据 viewport height 或 content size 动态调整。
- child surface z-index 仅 `parentZIndex + 10`,缺少同层排序策略。

方案:引入 `resolveSurfaceLayout(surface, layoutBudget)` 做动态 placement。

### 6. Timeline 稳定 item 和 timeline-owned state

> **状态:已完成。** 2026-06-04 审查更新。

- `TimelineItem.id` 使用稳定派生:`msg:${messageId}`、`tool:${toolCallId}`(`packages/host-tui/src/timeline/timeline-builder.ts:4` 的 stable ID policy)。
- Reducer 在每个事件(`user_submitted`、`assistant_delta`、`tool_call_started/ended`)中同步维护 `state.timeline.items`(见 `packages/host-tui/src/state/reducers/handleStream.ts:39`、`handleInput.ts`、`handleToolCalls.ts`)。
- `turn_finished` 改为 reconcile 而非 replace,保留已有 messageId 避免 ID 跳变。
- `session_resumed` 通过 `initTimelineItems()` 初始化。
- `App.tsx` 从 `state().timeline.items` 读取,不再每次 render 调用 `buildTimelineItems()`。

**剩余风险:**

- `buildTimelineItem()` 中 `createdAt: Date.now()` 在 `turn_finished` rebuild timeline items 时会生成新时间戳(不影响功能,但时间不一致)。
- `turn_finished` 中 `reconciled.map(buildTimelineItem)` 会完全重建 timeline items 数组(虽然 ID 稳定,但对象引用会变,可能触发不必要的 SolidJS 重渲染)。

方案:`turn_finished` 后不重建整个 timeline items,只对变化的 item 做增量更新。

### 7. Notifications 历史 UI 和 producers

> **状态:已完成。** 2026-06-04 审查更新。

- NotificationCenter 有完整 TTL / dedupe / history / mark read / clear / filter API。
- Producers 已接入关键事件路径:ActionService 在 stream error、stream abort、model switch、thinking change、session resume 时产出通知(通过 `onNotify` callback)。
- `/notifications` 和 `/noti` 可用 ReadOnlyListSurface 浏览,Enter 可 mark read。Hints 已改为 keymap 生成。

**剩余问题:**

- history UI 未暴露 clear/filter/expand detail 操作。
- 不接受 slash args(`/notifications error` 等)。

方案:为 notification surface 增加动作按钮或 keyboard shortcuts(如 `c` 清空、`f` 过滤),并在 `/notifications` 命令中解析 args。

### 8. Selectors 共享度已有,但行为不完整

共享 `SelectListView`、`SelectorShell`、`selector-controller` 已存在,model/thinking/resume/settings 也使用 controller 注册。但缺口包括:

- selector filter row 的键盘输入与 focused surface 未完整闭环。
- `SelectListView` 使用固定 width 80 计算 layout,未用实际 terminal/surface width。
- label/description 没有严格列宽、middle truncate、description threshold。
- hint row 多处仍传硬编码字符串。
- no-match、scroll counter 和 visible window 有基础,但未完全满足 "never overlap" 的布局约束。

方案:

1. `SelectorShell` 接收 resolved `SurfaceLayout`,向 `SelectListView` 传真实 width/height。
2. 把 filtering 纳入 `SelectorController.handleText()`,由 focus owner 接收 printable input。
3. 实现 `formatSelectorRow(item, layout)`,统一 primary/description/badge 截断。
4. selector hints 由 `KeymapManager.keyHint()` 生成。

### 9. Autocomplete provider 系统

> **状态:基本完成。** 2026-06-04 审查更新。

- Provider 架构已落地:`packages/host-tui/src/autocomplete/` 包含 `AutocompleteProvider` 接口、`SlashCommandAutocompleteProvider`、`FileAutocompleteProvider`(`@path` 补全)、`CombinedAutocompleteProvider`。
- `TuiController` 同时暴露同步 `getAutocomplete()`(供 interceptor 用)和异步 `getAutocompleteAsync()`(供 Editor `createResource` 用)。
- Editor 已通过 `createResource` 接入 CombinedAutocompleteProvider,支持 `/` 和 `@` 触发。

**剩余问题:**

- Autocomplete 仍 inline 渲染在 Editor 组件内(`<CommandAutocomplete>`),未通过 `SurfaceManager` 打开 anchored surface。
- 缺 `CommandArgumentAutocompleteProvider`。
- 未在 autocomplete surface 的 role/mount 上做声明(即 `role: "autocomplete"`, `preferredMount: "anchored"`, `anchorId: "editor"`)。

方案:当 provider 返回 suggestions 时,runtime 打开/更新 autocomplete surface;Editor 只触发 provider 查询,不再直接渲染 `<CommandAutocomplete>`。autocomplete surface 通过 SurfaceContentRegistry 渲染 UI。

### 10. Hints 硬编码

> **状态:已完成。** 2026-06-04 审查更新。

所有可见 key hints 已替换为 `KeymapManager.formatHintLine()` 生成:

- `SurfaceContentRegistry` 中 notifications/hotkeys/help/session-tree/changelog/session-info 使用 `browseHints` / `notifHints`。
- `SettingsSelector`、`ModelSelector`、`ThinkingSelector`、`ResumeSelector` 使用 `controller.keymap.formatHintLine()`。

Editor placeholder(`/model  /thinking  /resume  /exit`)仍为硬编码字符串,但这是 discoverability hint 而非 key hint,属于不同类别。

方案:可选地将 Editor placeholder 也改为从斜杠命令列表动态生成。

## 建议落地顺序(2026-06-04 更新)

前 7 项已全部落地。当前剩余工作按优先级:

1. **Autocomplete surface 化**:把 Editor 内 inline 渲染的 `<CommandAutocomplete>` 改为通过 SurfaceManager 打开 anchored surface,实现文档定义的 `role: "autocomplete"` + `preferredMount: "anchored"` + `anchorId: "editor"`。
2. **Keymap 文件加载**:接入 `~/.piko/keybindings.json` 和 `.piko/keybindings.json`,启动时 detectConflicts + notification 报告。
3. **Slot renderer 提取**:把 App.tsx 中的 `renderSlot()` 提取为独立文件或由 render-plan 携带 render 闭包。

已完成的补充项:

- ✅ `app.tools.expand` 实现 `timeline_toggle_all_tools` event + reducer handler
- ✅ `/fork` → fork-session surface(加载 branch entries,选择后调用 `host.forkSession()`)
- ✅ `/scoped-models` → 复用 model selector
- ✅ turn_finished 增量更新 timeline items(保留不变项的 object identity)
- ✅ FocusManager.onChange → store 同步 focus state
- ✅ `dispatch` 加入 CommandContext(命令可直发 store events)

## 验收建议

- 新增 `packages/host-tui/src/timeline/*.test.ts`:验证 user/assistant/tool streaming 使用 stable ids,manual scroll 时 pending count 增长,jump latest 清零。
- 新增 `keymap-manager.test.ts`:验证默认绑定、override resolution、conflict detection、display text。
- 新增 `focus-manager.test.ts`:验证 child surface 关闭返回 parent,parent close 关闭 descendants,interceptor 优先级和 bubbling。
- 新增 `surface-resolver.test.ts`:覆盖 narrow/wide、large selector、side-drawer、replace-slot、destructive confirm。
- 对 OpenTUI renderer 做轻量 smoke:`/model`、`/notifications`、slash unknown、autocomplete tab accept、Esc close restore editor。
