# Pointer Input (click / hover / wheel)

> Status: reviewed
>
> Package: `piko-tui` (product) · `piko-tui-layout` (hit contract)

## Overview

Mouse input is dispatched through the same per-frame hit map that the layout
already produces: a pointer event becomes a coordinate, the coordinate
resolves to a region + element, and the element maps to the **same actions the
keyboard uses**. Wheel scrolling and cursor placement are handled directly by
the owning region. Hover is tracked as state; drag and hover visuals are
non-goals for this feature.

## Problem

The pointer-ready layout contract exists, but there is no input path that
consumes it. Without one, the hit map is dead data and users cannot click
workflow choices, scroll the stream, place the editor cursor, or dismiss
notifications with the mouse.

## User journeys

1. A tool interaction dialog is open. The user clicks a choice row and the
   selection moves to it; clicking again (or clicking a Submit tab/row)
   advances or submits exactly like Enter.
2. An approval dialog is open. The user clicks "Accept for session" and the
   approval resolves with the session-scoped grant. Clicking the dialog
   background does nothing — the dialog is blocking.
3. The user scrolls the wheel over the conversation and the timeline scrolls.
4. The user clicks the composer and the text cursor moves to the clicked
   column.
5. A warning notice is visible; clicking it dismisses it.

## In scope

- Left-click dispatch over the hit map: workflow choices/tabs/submit, approval
  decisions, notice dismiss, suggestion accept, composer cursor placement.
- Wheel scroll over the Stream region.
- Hover tracking (no visual feedback yet).
- Mouse capture lifecycle (enable on enter, disable on exit).

## Out of scope

- Drag (text selection, scrolling by drag).
- Hover visuals / hover styling.
- Right-click and middle-click actions.
- Touch events.

## Layout

Hit zones come from the existing composition:

```text
plane
  Stream   → wheel scroll (click: no-op)
  Notice   → click dismiss
  Suggest  → click row accepts that suggestion
  Composer → click focuses and moves the text cursor
modal surfaces
  Decide   → click elements resolve like keyboard shortcuts;
             clicks on the dialog background are ignored
  Browse/Select → surface-level hits only (no per-element actions yet)
```

## Behavior / interactions

### Click resolution rules

1. Build the per-frame hit map; resolve `(x, y)`.
2. If the hit is inside a Decide surface:
   - Approval choice row → the matching decision (Accept once / session /
     workspace / permanent / Decline).
   - Tool Interaction choice row → select that choice, then act like Enter
     (enter inline input, advance, or submit per the workflow state).
   - Tool Interaction tab → jump to that question (or the Submit step).
   - Submit tab/row → submit.
   - Surface default (`element: None`) → ignored (blocking dialog).
3. If the hit is in the plane:
   - Notice → clear the visible notification.
   - Suggest row → accept that suggestion.
   - Composer → focus the editor (when no modal is open) and move the cursor
     to the clicked column.
   - Stream → no-op on click; wheel scrolls.
4. Any other hit (chrome, unknown) → no-op.

### Wheel

- Scroll up/down over Stream scrolls the timeline by a wheel step (3 rows).
- Wheel elsewhere is ignored.

### Hover

- Mouse movement updates `AppState::hovered` to the resolved
  `(region, element)`; no visual change in this feature.

### Focus / modal interaction

- A pending Decide surface is a pointer barrier: clicks outside its host rect
  are ignored (they cannot reach the plane or other surfaces).
- Browse/Select surfaces capture their area; plane click targets are only
  reachable when no modal owns the coordinate.

## Configuration

No user-facing configuration. Mouse capture is always enabled while the TUI
runs and disabled on exit.

## Non-goals

- This feature does not implement drag or hover styling.
- It does not add per-element actions to Browse/Select surfaces yet.

## Acceptance criteria

- [x] Clicking an approval choice row resolves the corresponding approval
      decision; clicking the dialog background resolves nothing.
- [x] Clicking a tool-interaction choice row selects it and advances per the
      workflow state; clicking a tab jumps to that question; clicking Submit
      submits.
- [x] Wheel over Stream scrolls the timeline; wheel elsewhere is ignored.
- [x] Clicking the composer (no modal open) moves the editor cursor to the
      clicked column.
- [x] Clicking a visible notice clears it; clicking a suggestion row accepts
      it.
- [x] Hover updates `AppState::hovered` without side effects.
- [x] Mouse capture is enabled on terminal enter and restored on exit.
- [x] Pointer dispatch is covered by unit tests (zone mapping, modal barrier,
      wheel, cursor, notice).

## Implementation status

Landed:

- `piko-tui-layout`: no changes needed — the hit map already supported the
  pointer path.
- `piko-tui`: plane hit specs in `build_surface_hitmap` (Stream, Notice,
  Suggest rows, Composer), `HitId` plane variants, `ToolInteractionAction::
  GotoStep`, `InteractiveWorkflow::goto_step`, `Editor::move_to_column`,
  `AutoComplete::select_index`, `AppState::hovered`.
- `piko-tui`: `input/pointer.rs` routes click / hover / wheel to
  keyboard-equivalent actions; `TerminalGuard` enables and restores mouse
  capture; `main.rs` feeds `CrosstermEvent::Mouse`.
- Tests: `app/tests/pointer_tests.rs` (10 cases).

Open product question 1 (hover select-on-hover) stays open by default: hover
is tracked but does not move keyboard selection.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Click on workflow choice | Select + Enter semantics | Mirrors keyboard; one click to answer |
| Click outside a Decide dialog | Ignored | Dialog is blocking (matches keyboard barrier) |
| Wheel step size | 3 rows | Comfortable vs 1 (slow) / 8 (page) |
| Hover behavior | Track only, no visuals | Foundation for later styling; avoids surprise selection |
| Composer clickable when modal open | No | Modal owns focus; composer not an input target |

## Open questions

1. Should hover on a workflow choice move keyboard selection (select-on-hover)?
   (Default: no — keep keyboard and pointer selection independent until
   visual feedback exists.)
2. Should wheel scroll Select/Browse list surfaces in a later pass?

## Reference evidence

- Hit contract: `packages/tui-layout/src/hitmap.rs`.
- Per-frame hit map: `packages/tui/src/layout/mod.rs`
  (`build_surface_hitmap`).
- Modal authority: `packages/tui/src/app/impls.rs` (`modal_surface` /
  `pending_decide`).
- Event loop: `packages/tui/src/main.rs`.
