# Concepts

piko TUI 的统一概念体系。所有代码、文档、注释必须使用以下词汇。

## 三层模型

```
┌─────────────────────────────────────────────────┐
│  Slot                                            │
│  Layout 层的位置。由 build_constraints 分配。     │
│  类型：LayoutSlots                               │
├─────────────────────────────────────────────────┤
│  Panel                                           │
│  填充 slot 的可见元素。所有 UI 都是 panel。        │
│  目录：panels/                                   │
│  两种：widget panel / overlay panel               │
├─────────────────────────────────────────────────┤
│  Component                                       │
│  可复用的底层构件。panel 内部使用 component 渲染。  │
│  目录：components/                               │
└─────────────────────────────────────────────────┘
```

## 1. Slot（槽位）

Layout 中的位置。命名：A、B、C、D'、D、E。

| Slot | 约束 | 语义 |
|------|------|------|
| A    | Fill(1) | 主内容区（timeline 或 full overlay） |
| B    | Length(h) | Agent panel |
| C    | Length(1) | Notification row（条件性） |
| D'   | Length(s) | Suggestions（条件性） |
| D    | Length(5) / Fill(1) | Editor 或 partial overlay |
| E    | Length(1) | Bottom bar（始终） |

Slot 是纯 layout 概念，不关心谁填它。

## 2. Panel（面板）

填充 slot 的可见元素。分为两种。

### Widget panel

始终占据固定 slot，不替换其他 panel。

| Panel            | Slot | 文件                  |
|------------------|------|-----------------------|
| Timeline         | A    | `panels/timeline.rs`  |
| AgentPanel       | B    | `panels/agent.rs`     |
| NotificationRow  | C    | `panels/notification_row.rs` |
| Suggestions      | D'   | `panels/suggestions.rs` |
| Editor           | D    | `panels/editor.rs`    |
| BottomBar        | E    | `panels/bottom_bar.rs` |

### Overlay panel

临时替换某个 widget panel。有自己的 FocusTarget + InputPolicy。

| Panel              | 替换           | Placement | 文件                      |
|--------------------|----------------|-----------|---------------------------|
| CommandPalette     | Editor         | Partial   | `panels/command_palette.rs` |
| ModelSelector      | Editor         | Partial   | `panels/model_selector.rs`  |
| SettingsPanel      | Editor         | Partial   | `panels/settings.rs`        |
| ApprovalPanel      | Editor 前插入   | —         | `panels/approval.rs`        |
| SessionList        | A+B+C+D        | Full      | `panels/session_list.rs`    |
| TreePanel          | A+B+C+D        | Full      | `panels/tree.rs`            |
| HelpPanel          | A+B+C+D        | Full      | `panels/help.rs`            |
| StatusPanel        | A+B+C+D        | Full      | `panels/status.rs`          |

注：ApprovalPanel 不替换任何 panel，而是在 AgentPanel 和 Editor 之间插入一个 `Fill(1)` slot。

### Panel 的通用 trait（待实现）

```rust
trait Panel {
    /// 渲染自己到给定区域
    fn render(&self, frame: &mut Frame, area: Rect, app: &AppState);
}
```

## 3. Component（构件）

Panel 内部使用的可复用渲染单元。不直接对应 slot。

| Component       | 说明                         | 被哪些 panel 使用           |
|-----------------|------------------------------|-----------------------------|
| FilterableList  | 可过滤 + 键盘导航的列表        | 所有 overlay panel          |
| InfoPanel       | 只读段落展示（Paragraph）      | HelpPanel, StatusPanel      |
| ConfirmDialog   | 居中确认弹窗                   | ApprovalPanel, ForkConfirm  |
| FormPanel       | 表单输入                       | LoginPanel, RenamePanel     |

## 4. 其他概念

| 术语              | 定义                                                |
|-------------------|-----------------------------------------------------|
| LayoutMode        | Chat / PartialOverlay / FullOverlay / Approval      |
| FocusTarget       | 当前焦点所在 panel（Editor / CommandPalette / …）    |
| Placement         | overlay panel 占据的位置：Full（替换 A+B+C+D）或 Partial（替换 D） |
| InputPolicy       | Capture（阻断按键传向 Editor）或 Passive（透传）      |
| Notification      | 用户通知，有 Info / Warning / Error 三级              |
| Floaters          | ❌ 已废弃。所有可见元素必须作为 panel 参与 layout     |

## 5. 目录结构

```
packages/tui/src/
├── panels/           # 所有 panel（widget + overlay）
│   ├── mod.rs
│   ├── agent.rs
│   ├── approval.rs
│   ├── bottom_bar.rs
│   ├── command_palette.rs    # was commands.rs
│   ├── editor.rs             # 从 render.rs 提取
│   ├── help.rs
│   ├── model_selector.rs     # was models.rs
│   ├── notification_row.rs
│   ├── session_list.rs       # was sessions.rs
│   ├── settings.rs
│   ├── status.rs
│   ├── suggestions.rs        # 从 render.rs 提取
│   ├── timeline.rs
│   └── tree.rs
├── components/        # 可复用底层构件
│   ├── mod.rs
│   ├── filterable_list.rs
│   ├── info_panel.rs
│   └── confirm_dialog.rs
├── config/            # TUI 配置
│   ├── mod.rs
│   └── bottom_bar.rs
├── input/             # 输入系统
│   ├── mod.rs
│   ├── completion.rs
│   ├── editor.rs
│   ├── focus.rs
│   └── keymap.rs
├── app/               # 应用状态 + 事件处理
│   └── ...
├── layout.rs          # Slot 布局引擎
├── render.rs          # 顶层渲染调度
├── notification.rs    # 通知中心（非 UI）
└── ...
```

## 6. 命名规则

- **Panel struct**：`XxxPanel` 或 `XxxRow`（单行 panel）
  - 例：`AgentPanel`、`NotificationRow`、`BottomBar`
  - overlay panel 不要 `Overlay` 后缀：`CommandPalette` 而非 `CommandsOverlay`
- **Component struct**：描述性名称，不加后缀
  - 例：`FilterableList`、`ConfirmDialog`
- **文件名**：`snake_case`，跟 struct 名对应
  - 例：`agent.rs` → `AgentPanel`，`bottom_bar.rs` → `BottomBar`
