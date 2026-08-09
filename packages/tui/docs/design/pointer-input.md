# Design: Pointer Input (click / hover / wheel)

> Status: accepted
>
> PRD: [`../features/pointer-input.md`](../features/pointer-input.md)

## Goal

Consume the per-frame hit map with real mouse events: clicks resolve to the
same actions as the keyboard, wheel scrolls the stream, composer clicks place
the text cursor, and hover gives soft feedback on actionable targets. No drag.

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

`route_pointer` owns terminal-event normalization, modal-layer authority, and
Region-to-component delegation. Each component interprets its element through
the product-layer `PointerComponent` contract, may mutate state it owns, and
returns keyboard-equivalent `Action`s for the normal effect pipeline. See
[component-interaction.md](component-interaction.md).

### Hover paint

Hover identity remains product state as `(Region, Option<HitId>)`. Composition
filters it into layout-provided `InteractionState<HitId>` for the region being
painted. The owning workflow, suggestion, notice, or editor renderer applies
its semantic theme token in its normal paint path. This keeps geometry and the
generic render-state contract in `piko-tui-layout`, while selection/hover
precedence stays with the component that owns the business state.

- Choice, tab, submit, suggestion, and notice targets receive `bg_hover`.
- Composer does not paint hover feedback. It keeps its normal `bg_elevated`
  body and focus-owned prompt border; click still places the caret.
- Stream, surface defaults, stale targets, and plane targets hidden by a modal
  receive no feedback.
- A target that is also keyboard-selected keeps its selected styling; hover
  does not mutate `selected_idx`.

No visual state is added to `piko-tui-layout`, and the generic `Component`
contract does not change.

### Plane hit specs

`build_surface_hitmap` gains plane-region entries:

| Region | Element | Geometry |
|---|---|---|
| Stream | `HitId::Stream` | whole stream rect (wheel fallback) |
| Timeline tool | `HitId::TimelineTool(i)` | render-plan block rect clipped to viewport |
| Notice | `HitId::Notice` | whole notice rect |
| Suggest | `HitId::Suggest(i)` | shared selectable viewport rows, preserving source index |
| Composer | `HitId::Composer` | whole composer rect |

Plane entries are `z = 0`; modal surfaces (already in the map) win overlap.
The router also compares a hit's layer with the active top modal layer, so
clicks outside a partial modal cannot fall through to the plane.

### HitId extensions

```rust
pub enum HitId {
    Stream,
    Composer,
    Notice,
    Suggest(usize),
    TimelineTool(usize), // visible Timeline component source index
    Row(usize),          // owning component's stable source-row index
    TextInput,           // owning component's editable field
    Content,             // scrollable read-only viewport
    Close,               // pane close affordance
    Mode(usize),         // pane title mode-strip option
    Tab(usize),          // existing
    Choice { .. },       // existing
    Submit,              // existing
}
```

### Component-owned element mapping

| Owner | Hit | Result |
|---|---|---|
| Approval | `Choice { choice: i }` | `ApprovalAction::Respond(decision(i))` |
| ToolInteraction | `Choice` / `Tab` / `Submit` | Matching workflow action(s) |
| NotificationCenter | `Notice` | `NotificationAction::Clear` |
| AutoComplete | `Suggest(i)` | Click selects then accepts; wheel only moves selection |
| Editor | `Composer` | Move cursor from local hit coordinate |
| Timeline | wheel over `Stream` or tool | `TimelineAction::ScrollUp/Down(3)` |
| Timeline | click `TimelineTool(i)` | `TimelineAction::ToggleTool(i)` |
| Select/active Browse owner | `Row(i)` | Select source row, then `SurfaceAction::Confirm` |
| MCP | `Row(i)` | Select only; no host effect |
| Processes | `Row(i)` | Select + `Confirm`, preserving arm/confirm state |
| Diagnostics | wheel on surface | `SurfaceAction::SelectPrev/Next` |
| SummaryPrompt | workflow element | Select/goto + `SurfaceAction::Confirm` |
| Auth API-key form | `TextInput` | Place the `TextBox` cursor |
| Settings | `Close` | `SurfaceAction::Close` |
| Sessions / Tree | `Mode(i)` | Apply that surface's scope/filter mode |

Surface-default hits are delegated but current components treat them as no-op.

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
`AutoComplete` delegates row selection and viewport geometry to
`SelectableList`; it owns only provider data and the accept-suggestion action.
impl AppState {
    pub hovered: Option<(Region, Option<HitId>)>; // updated on Moved
}
```

## Validation

Unit tests in `app/tests/pointer_tests.rs`:

1. Approval choice click → matching decision; background click → no action.
2. Tool interaction choice click → `Choice` + `Submit`; tab click →
   `GotoStep`; submit click → `Submit`.
3. Wheel over Stream → timeline scroll action; wheel over selectable surfaces
   and Suggest → selection movement; unsupported regions → no action.
4. Composer click (no modal) → editor cursor moves to the column.
5. Notice click → clear; Suggest row click → accept that suggestion; a
   viewport-shifted Suggest hit retains its source candidate index.
6. `Moved` updates `AppState::hovered` without actions.
7. Actionable hovered rects use semantic hover tokens; selected targets keep
   selected paint, and non-actionable hits remain unchanged.
8. Timeline tool block click toggles only that block; viewport clipping and
   inter-component gaps remain exact.

Run:

```text
cargo fmt --all
cargo test -p piko-tui-layout
cargo test -p piko-tui
cargo clippy --workspace --all-targets -- -D warnings
```

## Non-goals (this design)

- Drag handling, right/middle buttons, touch.
- Double-click and pointer-specific business commands; components reuse the
  existing keyboard action vocabulary.

## Implementation status

Landed as designed:

- Plane hit specs in `build_surface_hitmap`; `HitId::{Stream, Composer,
  Notice, Suggest(usize)}`.
- `input/pointer.rs` (click / hover / wheel → `Vec<Action>` or direct state
  updates), mouse capture in `TerminalGuard`, `CrosstermEvent::Mouse` branch
  in `main.rs`.
- New APIs: `GotoStep`, `goto_step`, `move_to_column`, `select_index`,
  `AppState::hovered`.
- Component-local hover paint: workflow, autocomplete, notice, and editor
  consume `InteractionState<HitId>`; selected and non-actionable targets paint
  no hover override.
- `piko-tui-layout`: `ComponentHit`, `PointerGesture`, `InteractionState`, and
  top-layer hit APIs. `ui/interaction.rs` retains only the Action-producing
  `PointerComponent` product contract.
- `SurfaceId::outside_click_policy`: dismissible surfaces map outside clicks
  to keyboard Close; blocking Dock surfaces consume them.
- Shared selectable row geometry covers filtered, grouped, and viewport-shifted
  rows; title close/mode affordances use Pane-derived geometry.
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
