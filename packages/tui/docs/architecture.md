# piko TUI — 核心系统架构设计

> 基于 PRD 功能需求，结合 ratatui 0.29 原生能力的三层核心系统设计。
> 版本：2026-06-29

---

## 1. 三层架构总览

```
┌──────────────────────────────────────────────────────────┐
│                    Input Layer                            │
│  KeyEvent → [Global Esc/Enter] → [Focus Owner] → [...]   │
│  优先级链：全局快捷键 > 焦点 Panel > Editor > discard     │
├──────────────────────────────────────────────────────────┤
│                    Focus Layer                            │
│  FocusStack: LIFO 栈                                     │
│  每个 target 带 input_policy: Capture | Passive          │
│  push → 打开 panel  ;  pop → 关闭 panel                  │
├──────────────────────────────────────────────────────────┤
│                    Layout Layer                           │
│  constraints: Vec<Constraint> = f(LayoutMode)            │
│  mode 驱动，纯函数，Flat Panel（无嵌套）                   │
│  每个 panel 是布局中的一个独立 Row                         │
└──────────────────────────────────────────────────────────┘
```

三层单向依赖：**Input → Focus → Layout**。Layout 只依赖 mode，不知道焦点是谁；Focus 只管理栈，不知道按键怎么路由；Input 只看栈顶决定交付。

---

## 2. Layout 层

### 2.1 设计原则

| 原则 | 说明 |
|------|------|
| **Flat Panel** | 所有 panel 纵向平铺，不嵌套。不存在 "状态栏包含智能体面板" 这种结构——AgentPanel 和 NotificationRow 是布局中两个独立行。 |
| **mode 驱动** | 布局是 `LayoutMode` 的纯函数。同一 mode 下 panel 数量固定、顺序固定。 |
| **显隐 = 移除** | 不可见的 panel 从 `Vec<Constraint>` 中移除（不分配高度），不是隐藏（`is_hidden` + 0 高度）。 |
| **替换 = 同位置交换** | Partial Panel 替换 Editor 位置；Full Panel 替换 Timeline+Agent+Notif+Editor 四个位置。 |
| **BottomBar 铁律** | 任何 mode 下 BottomBar 都是最后一个 constraint，始终渲染。 |

### 2.2 LayoutMode 状态机

```
                    ┌──────────────────────┐
     /model, Ctrl+L │                      │ Esc, Enter (确认)
     /thinking, ... │    PartialOverlay    │──────────────────┐
         ┌─────────│                      │──────────┐       │
         │         └──────────────────────┘          │       │
         ▼                                           ▼       ▼
┌─────────────────┐                         ┌─────────────────┐
│      Chat       │                         │      Chat       │
│  (初始状态)      │◄────────────────────────│  (恢复)         │
└────────┬────────┘     Esc / Enter          └─────────────────┘
         │
         │  /resume, /tree(narrow),
         │  /notifications, /help, /status
         ▼
┌──────────────────────┐
│                      │     Esc, Enter, q
│    FullOverlay       │──────────────────►  Chat
│                      │
└──────────────────────┘

┌──────────────────────┐
│                      │     Enter(Accept) / Esc(Decline)
│    Approval          │──────────────────►  Chat
│  (自动触发, 阻塞)     │
└──────────────────────┘
```

**三个核心 mode：**

| Mode | 触发条件 | Panel 序列 |
|------|----------|-----------|
| `Chat` | 默认，无 overlay | Timeline → AgentPanel → NotificationRow? → Editor → BottomBar |
| `PartialOverlay` | 打开 partial 面板 | Timeline → AgentPanel → NotificationRow? → **PartialPanel** → BottomBar |
| `FullOverlay` | 打开 full 面板 | **FullPanel** → BottomBar |
| `Approval` | 工具审批待处理 | Timeline → AgentPanel → ApprovalPanel → BottomBar |

> **注：** `Approval` 可视为 `PartialOverlay` 的变体——唯一区别是它阻塞 Enter/Esc 全局行为且强制捕获焦点。

### 2.3 Constraint 构建

```rust
/// 核心函数：LayoutMode → Vec<Constraint>
fn build_constraints(
    mode: LayoutMode,
    agent_height: u16,       // AgentPanel 动态高度（collapsed=1, expanded=N）
    has_notification: bool,  // 是否有可见通知
    editor_height: u16,      // Editor 高度（默认 5）
) -> Vec<Constraint> {
    let mut constraints = Vec::new();

    match mode {
        LayoutMode::Chat | LayoutMode::PartialOverlay { .. } => {
            // Slot A: Timeline（占剩余空间）
            constraints.push(Constraint::Fill(1));

            // Slot B: AgentPanel（动态高度，flexShrink=0）
            constraints.push(Constraint::Length(agent_height));

            // Slot C: NotificationRow（仅当有通知且 idle）
            if has_notification {
                constraints.push(Constraint::Length(1));
            }
        }
        LayoutMode::FullOverlay { .. } => {
            // Slot A: Full Panel（占剩余空间，替换 A+B+C+D）
            constraints.push(Constraint::Fill(1));
        }
        LayoutMode::Approval => {
            // Slot A: Timeline（缩小但可见）
            constraints.push(Constraint::Fill(1));
            // Slot B: AgentPanel
            constraints.push(Constraint::Length(agent_height));
            // Slot C: ApprovalPanel（占剩余空间）
            constraints.push(Constraint::Fill(1));
        }
    }

    // Slot D: Editor or Partial Panel
    match mode {
        LayoutMode::Chat | LayoutMode::Approval => {
            constraints.push(Constraint::Length(editor_height));
        }
        LayoutMode::PartialOverlay { .. } => {
            constraints.push(Constraint::Fill(1)); // 替换 Editor
        }
        LayoutMode::FullOverlay { .. } => {
            // Full Panel 已替换所有，不添加
        }
    }

    // Slot E: BottomBar（始终）
    constraints.push(Constraint::Length(1));

    constraints
}
```

**关键设计决策：**
- **Fill 约束**：Chat 模式只有一个 `Fill(1)`（Timeline），Partial 模式有两个（Timeline + PartialPanel，各 1:1），Full 模式只有一个（FullPanel）。ratatui 按权重自动分配剩余空间。
- **AgentHeight 动态**：由 `AgentPanel::height(app)` 计算——collapsed 1 行，expanded 根据计划步骤数 + 队列项数动态计算。
- **NotificationRow 条件**：仅在无 active turn 且有未读通知时加入 constraints。

### 2.4 渲染流程

```rust
fn render(frame: &mut Frame, app: &AppState) {
    let mode = app.layout_mode();
    let constraints = build_constraints(mode, AgentPanel::height(app), app.has_visible_notification(), 5);
    let slots = Layout::vertical(constraints).split(frame.area());

    let mut i = 0;

    // Slot A: Timeline or Full Panel
    match mode {
        LayoutMode::Chat | LayoutMode::PartialOverlay { .. } | LayoutMode::Approval => {
            app.timeline.render(frame, slots[i]);
        }
        LayoutMode::FullOverlay { .. } => {
            render_full_panel(frame, app, slots[i]);
            i += 1; // 跳过 A+B+C+D（被 Full Panel 替换）
            // 直接到 BottomBar
            BottomBar::render(frame, app, slots[i]);
            // 浮层（补全、通知弹窗）
            render_floaters(frame, app, frame.area());
            return;
        }
    }
    i += 1;

    // Slot B: AgentPanel（Chat / Partial / Approval）
    if mode != LayoutMode::FullOverlay { .. } {
        AgentPanel::render(frame, slots[i], app);
        i += 1;

        // Slot C: NotificationRow（条件性）
        if app.has_visible_notification() {
            NotificationRow::render(frame, slots[i], app);
            i += 1;
        }
    }

    // Slot D: Editor or Partial Panel
    match mode {
        LayoutMode::Chat => {
            Editor::render(frame, slots[i], app);
        }
        LayoutMode::PartialOverlay { .. } => {
            render_partial_panel(frame, app, slots[i]);
        }
        LayoutMode::Approval => {
            ApprovalPanel::render(frame, slots[i], app);
        }
        LayoutMode::FullOverlay { .. } => { /* handled above */ }
    }

    // Slot E: BottomBar
    BottomBar::render(frame, app, slots.last().unwrap_or(&Rect::default()));

    // 浮层（不参与 Layout，Clear + 绝对定位）
    render_floaters(frame, app, frame.area());
}

fn render_floaters(frame: &mut Frame, app: &AppState, area: Rect) {
    // CompletionPopup: 浮在 Editor 上方
    if app.has_suggestions() {
        CompletionPopup::render(frame, app, /* 基于 editor_area 计算 */);
    }
    // Notification 弹窗: 右上角
    if !app.notifications.items().is_empty() {
        NotificationToast::render(frame, app, area);
    }
}
```

### 2.5 渲染时的 layout mode 计算

```rust
impl AppState {
    fn layout_mode(&self) -> LayoutMode {
        // Approval 优先（阻塞所有其他模式）
        if !self.approvals.is_empty() {
            return LayoutMode::Approval;
        }

        let active = self.focus.active_mode();

        // 没有 overlay → Chat
        if active == AppMode::Chat {
            return LayoutMode::Chat;
        }

        // 根据 surface placement 决定
        match active.placement() {
            Some(SurfacePlacement::Full) => LayoutMode::FullOverlay { mode: active },
            Some(SurfacePlacement::Partial) => LayoutMode::PartialOverlay { mode: active },
            None => LayoutMode::Chat,
        }
    }
}
```

---

## 3. Focus 层

### 3.1 设计原则

| 原则 | 说明 |
|------|------|
| **LIFO 栈** | 打开 panel → push；关闭 panel → pop。栈底永远是 `Editor`。 |
| **单焦点** | 任何时候只有栈顶 target 是焦点所有者。 |
| **Input Policy** | 每个 target 声明 `Capture`（阻断向下传递）或 `Passive`（透传）。 |
| **无焦点漫游** | 不在 panel 之间 Tab 切换焦点。焦点只通过 push/pop 切换。 |

### 3.2 FocusTarget 定义

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusTarget {
    Editor,
    // Partial overlays (Capture)
    ModelSelector,
    ThinkingSelector,
    SettingsMenu,
    CommandPalette,
    SessionTree { narrow: bool },
    LoginPanel,
    RenamePanel,
    ForkConfirm,
    ApprovalPanel,
    // Full overlays (Capture)
    SessionResume,
    NotificationHistory,
    // Full overlays (Passive)
    Help,
    StatusDiagnostic,
}

impl FocusTarget {
    fn input_policy(&self) -> InputPolicy {
        match self {
            FocusTarget::Editor => InputPolicy::Passive,
            FocusTarget::Help | FocusTarget::StatusDiagnostic => InputPolicy::Passive,
            _ => InputPolicy::Capture,
        }
    }

    fn placement(&self) -> SurfacePlacement {
        match self {
            FocusTarget::Editor => SurfacePlacement::None,
            FocusTarget::SessionResume
            | FocusTarget::NotificationHistory
            | FocusTarget::Help
            | FocusTarget::StatusDiagnostic => SurfacePlacement::Full,
            _ => SurfacePlacement::Partial,
        }
    }
}
```

### 3.3 FocusStack

```rust
struct FocusStack {
    stack: Vec<FocusTarget>, // 栈底 = Editor, 栈顶 = 当前焦点
}

impl FocusStack {
    fn new() -> Self {
        Self { stack: vec![FocusTarget::Editor] }
    }

    fn push(&mut self, target: FocusTarget) {
        // 避免重复压入同一个 target
        if self.stack.last() != Some(&target) {
            self.stack.push(target);
        }
    }

    fn pop(&mut self) -> Option<FocusTarget> {
        // 栈底 Editor 不能 pop
        if self.stack.len() > 1 {
            self.stack.pop()
        } else {
            None
        }
    }

    fn top(&self) -> FocusTarget {
        *self.stack.last().unwrap_or(&FocusTarget::Editor)
    }

    fn clear_to_editor(&mut self) {
        self.stack.truncate(1);
    }

    fn is_blocking(&self) -> bool {
        self.top().input_policy() == InputPolicy::Capture
    }
}
```

---

## 4. Input 层

### 4.1 设计原则

| 原则 | 说明 |
|------|------|
| **优先级链** | Global > FocusOwner > Editor。链上第一个消费事件者胜出。 |
| **Capture 阻断** | FocusOwner 是 Capture 时，Editor 不接收任何按键。 |
| **Passive 透传** | FocusOwner 是 Passive 时，未处理的按键继续传给 Editor。 |
| **Global Esc 特殊** | Esc 始终全局处理，不受 Capture/Passive 影响。 |

### 4.2 优先级链

```
KeyEvent
    │
    ▼
┌──────────────────────┐
│ P1: Global Esc/Enter │  无论 focus 是谁，先检查全局行为
│                      │
│ Esc:                 │
│  1. Approval 待处理 → Decline
│  2. Capture panel   → CloseSurface (pop focus)
│  3. 补全可见         → CancelSuggestions
│  4. 流运行中         → Cancel (中断)
│  5. Editor 空 + 双击  → OpenTree
│                      │
│ Enter:               │
│  1. Approval 待处理 → Accept
│  2. Capture panel   → ConfirmSelection (传递给 panel)
│  3. 补全可见         → AcceptAndSubmitSuggestion
│  4. Chat mode       → Submit
└──────┬───────────────┘
       │ 未被 P1 消费
       ▼
┌──────────────────────┐
│ P2: Focus Owner      │  栈顶 FocusTarget 处理按键
│                      │
│ 如果 FocusOwner 是 Capture:
│   按键交给 panel 的内部 handler
│   (↑↓ 导航, 字符过滤, Enter 确认, Esc 关闭)
│   消费后终止，不传递到 Editor
│                      │
│ 如果 FocusOwner 是 Passive:
│   按键交给 panel 的内部 handler
│   未消费的按键继续传给 P3
└──────┬───────────────┘
       │ Passive 未消费 或 无 FocusOwner
       ▼
┌──────────────────────┐
│ P3: Editor           │  处理文本输入、光标移动、历史
│                      │
│ - InsertChar, Delete, Backspace
│ - CursorLeft/Right/LineStart/LineEnd
│ - HistoryPrev/Next
│ - Timeline 滚动 (PageUp/PageDown/End)
│ - 键盘命令 (Ctrl+L, Ctrl+P/N, Ctrl+R, Ctrl+O...)
└──────┬───────────────┘
       │ 未消费
       ▼
    discard (或 beep 通知)
```

### 4.3 route_key 伪代码

```rust
fn route_key(app: &AppState, key: KeyEvent) -> Option<Action> {
    // ═══ P1: Global Esc/Enter ═══
    if let Some(action) = handle_global_key(app, key) {
        return Some(action);
    }

    // ═══ P2: Focus Owner ═══
    let focus = app.focus.top();
    if focus != FocusTarget::Editor {
        if let Some(action) = handle_focus_key(app, focus, key) {
            return Some(action);
        }
        // 如果是 Capture，到此为止
        if focus.input_policy() == InputPolicy::Capture {
            return None; // 吞掉事件
        }
    }

    // ═══ P3: Editor ═══
    handle_editor_key(app, key)
}
```

### 4.4 Panel 内部输入处理

每个 Capture panel 需要实现的内部处理：

| Panel 类型 | ↑/↓ | PageUp/Down | Enter | Esc | 可打印字符 |
|-----------|------|-------------|-------|-----|-----------|
| FilterableList | 移动选中 | 翻页 | 确认选择 | 关闭 | 过滤文本追加 |
| ConfirmDialog | ←/→ 切换 | — | 确认 | 取消 | — |
| FormPanel | — | — | 提交 | 取消 | 字段输入 |
| InfoPanel (Passive) | — | — | 关闭 | 关闭 | — |

---

## 5. Panel 清单与归类

### 5.1 全量 Panel → Layout Slot 映射

```
Slot A: 主内容区
  ┌─ TimelineView          (Chat/Partial/Approval 时)
  └─ Full Panel            (FullOverlay 时)
       ├─ SessionResume
       ├─ SessionTree (narrow)
       ├─ NotificationHistory
       ├─ Help
       └─ StatusDiagnostic

Slot B: 智能体行 (仅在 Chat/Partial/Approval 模式)
  └─ AgentPanel

Slot C: 通知行 (仅在 Chat/Partial 模式，有条件)
  └─ NotificationRow

Slot D: 输入区
  ├─ Editor               (Chat 时)
  ├─ Partial Panel        (PartialOverlay 时)
  │    ├─ ModelSelector
  │    ├─ ThinkingSelector
  │    ├─ SettingsMenu
  │    ├─ CommandPalette
  │    ├─ SessionTree (wide)
  │    ├─ LoginPanel
  │    ├─ RenamePanel
  │    └─ ForkConfirm
  └─ ApprovalPanel        (Approval 时)

Slot E: 底栏 (始终)
  └─ BottomBar

浮层 (不参与 Layout, Clear + 绝对定位)
  ├─ CompletionPopup      (Chat, 浮在 Editor 上方)
  └─ NotificationToast    (右上角弹窗)
```

### 5.2 Panel → 底层 Component 映射

Panel 归类为 6 种底层组件：

| Component | ratatui 原生依赖 | 覆盖 Panel 数 |
|-----------|-----------------|--------------|
| **FilterableList** | `List` + `ListState` + `Block` | 10 |
| **InfoPanel** | `Paragraph` + `Wrap` + `Block` | 2 |
| **ConfirmDialog** | `Clear` + `Block` + `Paragraph` + `Span` | 3 |
| **FormPanel** | `Block` + `Paragraph` + `set_cursor_position` | 2 |
| **AgentPanel** | `Line`/`Span` + `Paragraph` | 1 |
| **InlineWidget** | 自定义 render | 4 |

#### FilterableList（覆盖 10 个 panel）

| Panel | 数据源 | 特性 |
|-------|--------|------|
| ModelSelector | hostd ModelList 响应 | 显示 `provider/modelId` + modelName，当前模型 * 标记 |
| ThinkingSelector | 静态列表 | off/low/medium/high |
| SettingsMenu | 静态列表 | 嵌套路由（主菜单 → 子页），含 toggle 状态 |
| CommandPalette | 静态列表 | 命令标题 + 描述 |
| SessionResume | hostd SessionList 响应 | 显示 session name + cwd + seq，当前会话 * 标记 |
| SessionTree | hostd Snapshot 数据 | 缩进层级、当前叶子标记、窄宽自适应 |
| NotificationHistory | 内存通知列表 | 严重级别图标、详情展开 |
| CompletionPopup | 动态补全 | 浮在 Editor 上方，位置动态计算 |
| ScopedModels | hostd 响应 | 与 ModelSelector 类似，项目级 |
| Hotkeys | 键位管理器 | 显示所有键位绑定及冲突 |

**FilterableList 通用 trait：**

```rust
trait Listable {
    fn primary(&self) -> Cow<'_, str>;
    fn detail(&self) -> Cow<'_, str>;
    fn is_active(&self) -> bool { false }
    fn matches(&self, filter: &str) -> bool;
}

struct FilterableList<T: Listable> {
    items: Vec<T>,
    state: ListState,          // ratatui 原生选中态
    filter: String,
    title: String,
    selected: usize,           // 原始 items 索引
    // 自适应
    max_visible: RangeInclusive<usize>,  // 3..=12
    show_detail_column: bool,  // 宽终端双列，窄终端单列
}
```

#### ConfirmDialog（覆盖 3 个 panel）

| Panel | 触发 | 选项 |
|-------|------|------|
| ForkConfirm | `/fork` | [Fork] [Cancel] |
| ApprovalPanel | 工具审批 | [Accept Once] [Accept Session] [Accept Workspace] [Decline] |
| DeleteConfirm | `/delete` | [Delete] [Cancel] |

#### FormPanel（覆盖 2 个 panel）

| Panel | 字段 | 验证 |
|-------|------|------|
| LoginPanel | Provider 选择 + API Key 输入 | Key 非空 |
| RenamePanel | 新会话名输入 | 非空 |

#### AgentPanel（1 个 panel，但最复杂）

PRD Section 7 全部需求：
- Collapsed: 单行 `● main  1 queued`
- Expanded: 头部 + 计划步骤 + 错误 + 队列项
- 同一时刻只有一个 agent 展开
- 状态图标：idle ● / running ◌(spinner) / failed ✗ / stopped ■
- 计划步骤：● 进行中 / ✓ 已完成 / ○ 待处理，颜色区分
- 队列项：↳ + 类型(steering/follow-up/next-turn) + 预览
- viewedAgent 强调色 + 实心圆点，背景暗淡色 + 空心圆点

#### InlineWidget（4 个 panel，各有独特逻辑）

| Widget | 复杂度 | 说明 |
|--------|--------|------|
| TimelineView | 高 | 7 种消息类型、流式输出、滚动状态机、折叠展开 |
| Editor | 中 | 多行文本、光标、历史、`/` `@` 触发补全 |
| BottomBar | 低 | 单行只读：session │ model │ tokens │ cwd │ git |
| NotificationRow | 低 | 单行：`│` 竖线 + 消息，按严重级别着色 |

---

## 6. 窄终端适配

终端宽度阈值：**64 字符**。

| 条件 | 适配行为 |
|------|----------|
| 宽度 < 64 | FilterableList 隐藏 detail 列，单列布局，max_visible 减半 |
| 宽度 < 64 | AgentPanel 折叠模式隐藏队列计数 |
| 宽度 < 64 | SessionTree 从 Full 变为 Partial 面板（不替换 Timeline） |
| 任意宽度 | CJK 宽字符（2列）和组合字符（0列）正确截断，不追加省略号 |

在 `build_constraints` 中不需要变——窄终端适配只在 **panel 内部渲染** 时根据 `area.width` 调整布局，不影响 constraint 数组。

---

## 7. 实施路线图

### Phase 1: 核心框架
1. 定义 `LayoutMode` enum + `build_constraints()` 函数
2. 重写 `render()` 为 flat 布局引擎
3. 实现 `BottomBar` widget
4. 拆分 `FocusTarget` + `InputPolicy`
5. 重写 `route_key()` 为三层优先级链

### Phase 2: 组件迁移
6. 提取 `FilterableList` 通用组件
7. 迁移现有 panel（Models, Sessions, Commands, Settings, Tree）到 FilterableList
8. 实现 `ConfirmDialog` 组件，迁移 Approval + Fork
9. 实现 `FormPanel` 组件，实现 Login + Rename

### Phase 3: 新功能
10. 实现 `AgentPanel` collapsed/expanded 完整功能
11. 实现 `ThinkingSelector`
12. 实现 `NotificationHistory`
13. 窄终端自适应

### Phase 4: 完善
14. `CompletionPopup` 浮层定位
15. `NotificationToast` 浮层
16. 键位绑定集成（提示行从 keymap 生成）
17. 主题颜色令牌

---

## 附录 A: 与 pi (TypeScript) TUI 的对比

| 概念 | pi (TS) | piko (Rust + ratatui) |
|------|---------|----------------------|
| 布局 | Ink `Box` + flexbox | ratatui `Layout` + `Constraint` |
| 焦点 | React 组件树 + focus | LIFO FocusStack |
| Panel | `useSurface()` hook | FocusTarget enum |
| 路由 | `SurfaceRouter` | FocusStack push/pop |
| 浮层 | Absolute positioning | `Clear` widget + 绝对 Rect |
| 状态管理 | React state + context | `AppState` struct (mutable) |
| 滚动 | Ink `Static` / viewport | ratatui `ListState` + 手动 scroll |

---

## 附录 B: ratatui Constraint 类型参考

| Constraint | 行为 | 使用场景 |
|-----------|------|----------|
| `Length(N)` | 精确 N 行，不伸缩 | BottomBar(1), Editor(5), NotificationRow(1) |
| `Fill(N)` | 按权重 N 分配剩余空间 | Timeline(1), PartialPanel(1) |
| `Min(N)` | 至少 N 行，可更多 | 备选方案（不建议用于 slot 布局） |
| `Percentage(P)` | 总高度 × P% | 备选方案（但终端高度变化时效果差） |
| `Ratio(A, B)` | A/(A+B) 比例 | 精确比例控制（如 Timeline:Partial = 3:2） |

**推荐：** `Fill` 用于弹性区域，`Length` 用于固定区域。`Min` 和 `Percentage` 仅作降级方案。
