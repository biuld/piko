# Pointer Input (click / hover / wheel)

> Status: reviewed
>
> Package: `piko-tui` (product) · `piko-tui-layout` (hit contract)

## Overview

Mouse input is dispatched through the same per-frame hit map that the layout
already produces: a pointer event becomes a coordinate, the coordinate
resolves to a region + element, and the element maps to the **same actions the
keyboard uses**. Wheel scrolling and cursor placement are handled directly by
the owning region. Hover gives soft visual feedback on actionable targets;
drag remains a non-goal.

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
  decisions, selectable surface rows, notice dismiss, suggestion accept,
  composer and form-input cursor placement.
- Wheel scroll over the Stream, scrollable Browse, and selectable list regions.
- Hover tracking and soft visual feedback for actionable targets already
  represented by the hit map.
- Mouse capture lifecycle (enable on enter, disable on exit).

## Out of scope

- Drag (text selection, scrolling by drag).
- Right-click and middle-click actions.
- Touch events.

## Layout

Hit zones come from the existing composition:

```text
plane
  Stream   → wheel scroll; tool blocks toggle independently
  Notice   → click dismiss
  Suggest  → click row accepts; wheel moves palette selection
  Composer → click focuses and moves the text cursor
modal surfaces
  Decide   → click elements resolve like keyboard shortcuts;
             clicks on the dialog background are ignored
  Browse/Select → row hits owned by each feature; click mirrors Enter
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
   - Stream background and read-only blocks → no-op on click; wheel scrolls.
   - Timeline tool block → toggle only that block's expanded details.
4. If the hit is inside a Browse/Select/Modal surface:
   - Action or navigation row → select the row and apply that surface's Enter
     semantics with one click.
   - Read-only list row → move selection only.
   - Process row → preserve the existing two-stage stop confirmation; the
     first click arms and a second click on the armed row confirms.
   - Summary prompt choice/tab/submit → use the embedded workflow semantics.
   - API-key or label input → place the owning text-box cursor.
5. Any other hit (chrome, unknown) → no-op.

### Wheel

- Scroll up/down over Stream scrolls the timeline by a wheel step (3 rows).
- Wheel over a selectable list moves its selection; wheel over Diagnostics
  scrolls its text viewport.
- Command and file suggestion palettes use the same selectable-list viewport:
  wheel moves selection without accepting, and row hits track the visible
  source indices after the viewport shifts.
- Wheel over fixed read-only content, form inputs, and chrome is ignored.

### Hover

- Mouse movement updates `AppState::hovered` to the resolved
  `(region, element)`.
- Actionable row targets use `theme.bg_hover`. Composer hover is intentionally
  inert: focus border and caret already communicate its input
  state; pointer click only places the cursor.
- Timeline tool blocks preserve their status background and use an accent
  disclosure affordance on hover.
- Hover is a preview only: it never moves keyboard selection. When hover and
  keyboard selection identify the same item, selected styling wins.
- Stream and surface-default (`element: None`) hits do not receive visual
  feedback. Hover never makes a non-clickable area look clickable.

### Focus / modal interaction

- A pending Decide surface is a pointer barrier: clicks outside its host rect
  are ignored (they cannot reach the plane or other surfaces).
- Browse, Select, and ordinary Modal surfaces close or step back on an outside
  click using the same action as keyboard Esc.
- While any modal owns focus, lower-layer plane targets are not reachable;
  outside clicks are dismissed or blocked by surface policy.

## Configuration

No user-facing configuration. Mouse capture is always enabled while the TUI
runs and disabled on exit.

## Non-goals

- This feature does not implement drag, double-click, right-click, middle-click,
  touch, text selection, or draggable scrollbars.
- Read-only Status content and bottom chrome do not pretend to be actionable.

## Acceptance criteria

- [x] Clicking an approval choice row resolves the corresponding approval
      decision; clicking the dialog background resolves nothing.
- [x] Clicking a tool-interaction choice row selects it and advances per the
      workflow state; clicking a tab jumps to that question; clicking Submit
      submits.
- [x] Wheel over Stream scrolls the timeline; wheel over selectable surfaces
      and Suggest moves selection; unsupported regions ignore it.
- [x] Clicking the composer (no modal open) moves the editor cursor to the
      clicked column.
- [x] Clicking a visible notice clears it; clicking a suggestion row accepts
      it.
- [x] Suggest palettes support wheel selection beyond one viewport, with hover
      and click mapped to the candidates actually visible.
- [x] Timeline tool hits follow block boundaries and viewport clipping; click
      toggles one tool while message blocks and inter-block gaps stay read-only.
- [x] Hover updates `AppState::hovered` without side effects.
- [x] Actionable hover targets render soft feedback without changing keyboard
      selection; selected styling wins on overlap.
- [x] Mouse capture is enabled on terminal enter and restored on exit.
- [x] Pointer dispatch is covered by unit tests (zone mapping, modal barrier,
      wheel, cursor, notice).
- [x] Every selectable surface exposes stable row hit regions and semantic
      hover feedback.
- [x] Row activation follows the owning surface's existing Enter behavior;
      Processes preserves two-stage confirmation and MCP remains read-only.
- [x] Diagnostics and list surfaces handle wheel input without fall-through.
- [x] SummaryPrompt and AuthSelector form input expose their component-specific
      pointer behavior.

## Implementation status

Landed:

- `piko-tui-layout`: no changes needed — the hit map already supported the
  pointer path.
- `piko-tui`: plane hit specs in `build_surface_hitmap` (Stream, Notice,
  Suggest rows, Composer), `HitId` plane variants, `ToolInteractionAction::
  GotoStep`, `InteractiveWorkflow::goto_step`, `Editor::move_to_column`,
  shared `SelectableList` selection/viewport helpers, `AppState::hovered`.
- `piko-tui`: `input/pointer.rs` normalizes click / hover / wheel, enforces
  top-modal authority, and delegates hits to component-owned pointer behavior;
  `TerminalGuard` enables and restores mouse capture; `main.rs` feeds
  `CrosstermEvent::Mouse`.
- `piko-tui`: composition passes the stored hover identity as generic
  `InteractionState` to the owning component; workflow, autocomplete, notice,
  and editor paint their own feedback.
- `piko-tui`: all selectable Browse/Select/Modal panels expose paint-aligned
  row hits; mode strips and Settings close affordance are clickable; Status
  remains intentionally read-only.
- Component-owned text fields in Auth, Tree label editing, ToolInteraction,
  and SummaryPrompt support pointer caret placement.
- Tests: pointer integration plus component geometry, safety, wheel, and
  viewport tests.

Hover remains independent from keyboard selection; select-on-hover is not part
of the interaction model.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Click on workflow choice | Select + Enter semantics | Mirrors keyboard; one click to answer |
| Click outside a Decide dialog | Ignored | Dialog is blocking (matches keyboard barrier) |
| Wheel step size | 3 rows | Comfortable vs 1 (slow) / 8 (page) |
| Hover behavior | Soft preview, no select-on-hover | Signals clickability without moving the keyboard target |
| Composer clickable when modal open | No | Modal owns focus; composer not an input target |

## Open questions

None for the current pointer scope.

## Reference evidence

- Hit contract: `packages/tui-layout/src/hitmap.rs`.
- Per-frame hit map: `packages/tui/src/layout/mod.rs`
  (`build_surface_hitmap`).
- Modal authority: `packages/tui/src/app/impls.rs` (`modal_surface` /
  `pending_decide`).
- Event loop: `packages/tui/src/main.rs`.
