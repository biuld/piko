# AGENTS.md — tui crate context

## Architecture overview

The TUI is organized in three layers:

```
Slot   →  layout position (A/B/C/D/E). Pure layout concept.
Panel  →  what fills a slot. All visible UI. Directory: `panels/`.
Component → reusable building blocks used inside panels. Directory: `components/`.
```

### Slot (layout layer)

Positions in the constraint array allocated by `build_constraints()`. Slots
don't care what renders into them. Defined in `layout/`. Slot split uses the
full terminal frame; Timeline and AgentPanel apply a shared left/right inset
(`DEFAULT_HORIZONTAL_INSET`). Other panels stay edge-flush.

| Slot | Constraint      | Occupied by                                |
|------|-----------------|--------------------------------------------|
| A    | `Fill(1)`       | Timeline (widget) or full overlay panel    |
| B    | `Length(h)`     | AgentPanel                                 |
| C    | `Length(1)`     | NotificationRow (conditional)              |
| D'   | `Length(n)`     | Suggestions (conditional)                  |
| D    | `Length(5)` / `Fill(1)` | Editor (widget) or partial overlay panel |
| E    | `Length(1)`     | BottomBar (always)                         |

### Panel (UI layer)

Everything that fills a slot is a panel. Two kinds:

**Widget panel** — always in a fixed slot, never replaces another panel.

| Panel            | Slot | File                       |
|------------------|------|----------------------------|
| Timeline         | A    | `panels/timeline.rs`       |
| AgentPanel       | B    | `panels/agent.rs`          |
| NotificationRow  | C    | inline in `render.rs`      |
| Suggestions      | D'   | inline in `render.rs`      |
| Editor           | D    | `input/editor.rs` (logic) + inline in `render.rs` (render) |
| BottomBar        | E    | `panels/bottom_bar.rs`     |

**Overlay panel** — temporarily replaces a widget panel. Has its own
`FocusTarget` and `InputPolicy`.

| Panel              | Replaces   | Placement | File                       |
|--------------------|------------|-----------|----------------------------|
| CommandPalette     | Editor     | Partial   | `panels/command_palette.rs` |
| ModelSelector      | Editor     | Partial   | `panels/model_selector.rs`  |
| SettingsPanel      | Editor     | Partial   | `panels/settings.rs`        |
| ApprovalPanel      | inserts before Editor | — | `panels/approval.rs`   |
| SessionList        | A+B+C+D    | Full      | `panels/session_list.rs`    |
| TreePanel          | A+B+C+D    | Full      | `panels/tree.rs`            |
| HelpPanel          | A+B+C+D    | Full      | `panels/help.rs`            |
| StatusPanel        | A+B+C+D    | Full      | `panels/status.rs`          |

ApprovalPanel is special: it doesn't replace any slot — it inserts a new
`Fill(1)` row between AgentPanel (slot B) and Editor (slot D).

### Component (reusable primitive)

Reusable rendering units used inside panels. Not tied to a slot.

| Component       | Description                        | Used by                        |
|-----------------|------------------------------------|--------------------------------|
| FilterableList  | Filterable, keyboard-navigable list | All overlay panels             |
| InfoPanel       | Read-only paragraph display         | HelpPanel, StatusPanel         |
| ConfirmDialog   | Centered confirmation popup         | ApprovalPanel, ForkConfirm     |
| FormPanel       | Form input                          | LoginPanel, RenamePanel        |

## Naming conventions

- **Panel struct**: `XxxPanel` or `XxxRow` (single-line panel). Overlay panels
  do NOT use an `Overlay` suffix — `CommandPalette`, not `CommandsOverlay`.
- **Component struct**: descriptive name with no suffix: `FilterableList`,
  `ConfirmDialog`.
- **File names**: `snake_case`, matching the struct name: `agent.rs` contains
  `AgentPanel`, `bottom_bar.rs` contains `BottomBar`.

## Design rules

1. **No floaters.** Every visible element must be a panel assigned to a layout
   slot. No `Clear` + absolute positioning.
2. **Panels are structs.** Every panel implements its own `render()` method.
   (NotificationRow, Suggestions, and Editor rendering are currently inline in
   `render.rs` — pending extraction to dedicated panel files.)
3. **Layout is pure.** `build_constraints()` is a pure function of
   `LayoutMode` + dynamic measurements. It does not know about panels.
4. **Focus is LIFO.** `FocusManager` is a stack. Push to open a panel, pop to
   close. No tab-based focus roaming.
5. **Input has three priorities.** P1: global Esc/Enter → P2: focus owner →
   P3: editor. Capture panels consume events; passive panels pass through.

## Adding a new panel

1. Create the struct in `panels/<name>.rs`
2. Implement `render(&self, frame, area, app)`
3. Register its `AppMode` variant + `Placement` in `app/mod.rs`
4. Add its `FocusTarget` handling in `input/focus.rs`
5. Wire rendering into `render.rs` in the appropriate slot

## TUI config

TUI settings live under the `[tui]` section in hostd settings. The TUI fetches
them at startup via `Command::ConfigGet { namespace: "tui" }`. The config
module (`config/`) owns the schema and defaults. Hostd just stores the blob.

Current configurable items:
- `tui.bottom_bar.items` — which items to show and in what order

## Docs structure

```
docs/
├── features/        # Pure functional specs — "what" the user sees
│   ├── bottom-bar.md
│   ├── editor.md
│   └── themes.md
└── design/          # Implementation design — "how" it's built
    └── ...
```

### features/ — functional specs

Each file describes a single feature purely from the user's perspective:
behavior, layout, keyboard shortcuts, configuration, and non-goals. No code
blocks, no file paths, no internal data structures.

A feature doc is considered **reviewed** once it accurately reflects the
implemented behavior. Before that, it may live as a draft.

**When to write a feature doc:** after the feature is implemented and
behavior is stable. Write it to capture the contract, not to plan the build.

**Feature doc template:**

```markdown
# Feature Name

## Overview
(one paragraph — what it is, where it lives, what it does)

## Layout
(ascii diagram, size, borders, colors)

## Behavior / interactions
(keyboard shortcuts, state transitions, edge cases)

## Configuration
(settings keys, keybinding IDs, defaults)

## Non-goals
(what it deliberately does NOT do)
```

### design/ — implementation design

Each file describes a subsystem's architecture: data flow between crates,
responsibility boundaries, protocol types, and key design decisions. Code
pseudocode and protocol DTO sketches are appropriate here.

**When to write a design doc:** before implementing a cross-cutting subsystem
(slash commands, input routing, layout engine) where multiple modules or
crates need to agree on a contract.

### Flow

```
1. Design doc (if subsystem-level) → agree on contracts
2. Implement → prototype + iterate
3. Feature doc → capture the stable user-facing behavior as spec
```

Feature docs are the source of truth for what the TUI does. Design docs are
the rationale for how it does it.
