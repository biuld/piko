# Design: Pointer Input (click / hover / wheel)

> Status: draft
>
> PRD: [`../features/pointer-input.md`](../features/pointer-input.md)

## Goal

Consume the per-frame hit map with real mouse events: clicks resolve to the
same actions as the keyboard, wheel scrolls the stream, composer clicks place
the text cursor, and hover becomes observable state. No drag, no hover
styling.

## Architecture

### Mouse capture lifecycle

`TerminalGuard::enter` adds `EnableMouseCapture`; `exit` (and `Drop`) restores
`DisableMouseCapture`. No other terminal state changes.

### Event → action pipeline

```text
CrosstermEvent::Mouse(event)          (main.rs)
  → input::pointer::route_pointer(app, terminal_rect, event)
  → build_surface_hitmap(app, terminal_rect)     (per event; cheap)
  → HitMap::hit_test(event.column, event.row)    (0-based, ratatui coords)
  → Vec<Action>                                  (same actions as keyboard)
  → app.update(Msg::Action(a)) + run_effects
```

`route_pointer` may mutate `AppState` directly where no action exists
(composer cursor placement, suggestion selection, hover state); everything
else returns `Action`s so effects flow through the normal pipeline.

### Plane hit specs

`build_surface_hitmap` gains plane-region entries:

| Region | Element | Geometry |
|---|---|---|
| Stream | `HitId::Stream` | whole stream rect |
| Notice | `HitId::Notice` | whole notice rect |
| Suggest | `HitId::Suggest(i)` | content rows `area.y + 1 + i` |
| Composer | `HitId::Composer` | whole composer rect |

Plane entries are `z = 0`; modal surfaces (already in the map) win any
overlap, so a Decide host is a pointer barrier by construction.

### HitId extensions

```rust
pub enum HitId {
    Stream,
    Composer,
    Notice,
    Suggest(usize),
    Tab(usize),          // existing
    Choice { .. },       // existing
    Submit,              // existing
}
```

### Element → action mapping

| Hit | Action(s) |
|---|---|
| Approval `Choice { choice: i }` | `ApprovalAction::Respond(decision(i))` — fixed order Accept / AcceptSession / AcceptWorkspace / AcceptPermanent / Decline |
| ToolInteraction `Choice` | `ToolInteractionAction::Choice(i)` then `Submit` (Enter semantics) |
| ToolInteraction `Tab(q)` | `ToolInteractionAction::GotoStep(q)` (new; Submit sentinel = questions.len()) |
| ToolInteraction `Submit` | `ToolInteractionAction::Submit` |
| Surface default (`element: None`) | none |
| `Notice` | `NotificationAction::Clear` |
| `Suggest(i)` | select index `i` (new `AutoComplete::select_index`) then `EditorAction::AcceptSuggestion` |
| `Composer` (no modal) | `Editor::move_to_column(width, col)` (new) |
| `Stream` click | none |
| wheel over `Stream` | `TimelineAction::ScrollUp/Down(3)` |

### New APIs

```rust
impl ToolInteractionAction {
    GotoStep(usize), // jump to question or Submit (len) — new variant
}
impl InteractiveWorkflow {
    pub fn goto_step(&mut self, step: usize); // len → confirm_focused
}
impl Editor {
    pub fn move_to_column(&mut self, width: u16, col: u16); // display col → byte cursor
}
impl AutoComplete {
    pub fn select_index(&mut self, idx: usize);
}
impl AppState {
    pub hovered: Option<(Region, Option<HitId>)>; // updated on Moved
}
```

## Validation

Unit tests in `app/tests/pointer_tests.rs`:

1. Approval choice click → matching decision; background click → no action.
2. Tool interaction choice click → `Choice` + `Submit`; tab click →
   `GotoStep`; submit click → `Submit`.
3. Wheel over Stream → timeline scroll action; wheel elsewhere → no action.
4. Composer click (no modal) → editor cursor moves to the column.
5. Notice click → clear; Suggest row click → accept that suggestion.
6. `Moved` updates `AppState::hovered` without actions.

Run:

```text
cargo fmt --all
cargo test -p piko-tui-layout
cargo test -p piko-tui
cargo clippy --workspace --all-targets -- -D warnings
```

## Non-goals (this design)

- Drag handling, hover styling, right/middle buttons, touch.
- Per-element actions for Browse/Select surfaces.

## Implementation status

Landed as designed:

- Plane hit specs in `build_surface_hitmap`; `HitId::{Stream, Composer,
  Notice, Suggest(usize)}`.
- `input/pointer.rs` (click / hover / wheel → `Vec<Action>` or direct state
  updates), mouse capture in `TerminalGuard`, `CrosstermEvent::Mouse` branch
  in `main.rs`.
- New APIs: `GotoStep`, `goto_step`, `move_to_column`, `select_index`,
  `AppState::hovered`.
- `app/tests/pointer_tests.rs` covers approval decisions, workflow choice /
  tab / submit clicks, wheel zones, composer cursor, notice clear, suggestion
  accept, and hover tracking.

### Pane integration for Decide docks

`InteractiveWorkflow` renders standalone modals through `render_pane`:

- `PaneSpec` gains `fill` (opaque modal backdrop) and
  `PaneTitleAffix::Label` (e.g. `tool: bash`).
- `PaneSpec::content_rect` is the pure geometry shared by `render_pane` and
  workflow hit regions, so painted rows and click targets cannot drift.
- Approval uses title `Approval` + tool-name affix + shortcut hint footer;
  Tool Interaction uses title `Tool Interaction` + queue-position affix.
- The embedded summary-prompt path keeps its compact top-border rendering
  (no pane chrome) to avoid double frames inside the tree pane.
