# Modal Surfaces & Pointer-Ready Layout

> Status: reviewed
>
> Package: `piko-tui` (product contract) · `piko-tui-layout` (engine contract)

## Overview

The TUI shows one modal surface at a time over a stable workspace plane.
Surfaces are mounted by intent (Browse covers the body, Select/Dock sit in the
composer dock replacing it while active, Modal is a centered dialog), and the
modal that is **drawn** must be the surface that owns **focus and input**.
Layout is solved once per frame into a single rect map; that map is the only
geometry authority and is the source for pointer hit-testing.

## Problem

Today modal drawing and input routing come from two different authorities that
are only kept consistent by convention:

- **Drawing** is decided by `resolve_modal_surface`: host-priority surfaces
  (Approval, then Tool Interaction) win over the focused surface.
- **Input** is decided by the LIFO focus stack; global shortcuts run *before*
  the active surface gets to capture keys.

Because nothing enforces that these two agree, a pending Approval or Tool
Interaction modal can keep drawing while focus moves to a different (invisible)
surface, making the visible prompt unanswerable until focus pops back.

Two further gaps block a clean modal model:

- Decide surfaces paint without clearing or filling their host rect, so the
  dock can leak: background stream/composer cells and stale glyphs are visible
  inside the dock area.
- Geometry is solved into rects but never exposed for hit-testing; adding
  mouse interaction later would require reworking the layout contract unless
  the pointer-ready shape is fixed now.

## User journeys

1. A tool requests approval while the user is reading the stream. Approval
   replaces the composer dock above the stream, focus moves to it, and every
   key is captured until the user accepts or declines. Pressing a global
   surface shortcut such as F4 does **not** move focus to a hidden surface.
2. A tool asks a multi-question workflow. The user tabs between questions,
   edits an inline value, reaches the Submit step, and confirms. Esc at any
   step cancels and the turn continues with the cancelled response.
3. (Future) The user clicks a choice row in a workflow or a tab in a
   multi-question dialog, and the click resolves the same element that
   keyboard focus would have. Clicks outside the Decide dock are ignored
   because the prompt is blocking.

## In scope

- Mounting policy: exactly one visible modal at a time, selected by
  host-priority (Approval > Tool Interaction > active surface).
- Focus-modal consistency: while a modal is open, the drawn modal **is** the
  focused surface; no input path may target a surface that is not drawn.
- Blocking semantics for Decide surfaces: all keys are captured, the editor
  draft is preserved, and the panel stays visible after the user responds
  until hostd resolves the request.
- Rendering contract: surfaces paint only into rects produced by the solver;
  modal surfaces clear (and Decide surfaces fill) their host before painting.
- Pointer-ready layout contract: every painted cell maps to exactly one region
  through the solved frame plan; hit regions are derived, not hand-maintained;
  per-surface sub-regions (rows, tabs, buttons) are declared as data.

## Out of scope

- Mouse event handling, cursor rendering, drag, or hover feedback (a later
  pointer-input feature consumes the hit contract defined here).
- More than one visible modal at a time.
- Reskinning individual surfaces; the existing Browse/Select/Decide recipes
  keep their current visuals except for the Decide backdrop fix.

## Layout

```text
frame
  → split_shell (BottomBar chrome)
  → plane = Stream ▾ [Notice] ▾ [Suggest] ▾ Composer     (always solved)
  → modal (exactly one, by intent)
       Browse  → CoverBody      (plane not painted)
       Select  → ComposerBand   (height from content-row budget)
       Decide  → ComposerBand   (workflow content rows + pane chrome)
  → solve → FramePlan { plane rects, ordered layer rects }
  → paint: chrome → plane (unless Browse) → layers (top-down)
```

Decide docks render on an opaque backdrop in the composer slot; no stream,
composer, or stale glyphs show through.

## Behavior / interactions

### Modal selection and focus

- Host-priority order when multiple prompts are pending: Approval first, then
  Tool Interaction, then the focused surface.
- Opening a modal pushes focus; resolving it pops focus back to the previous
  surface. Pending-submission state (response sent, hostd not yet confirmed)
  keeps both the modal and its focus until `Resolved`.
- A modal in progress is an input barrier: global shortcuts that open other
  surfaces must not steal focus from a Decide modal.

### Decide surfaces

- All keys are captured while the dialog is focused; the editor does not
  receive input and its draft is preserved.
- Approval: Enter accepts once, `a`/`w`/`p` scope the grant, Esc declines.
  Plain letter keys must not fire while Ctrl/Alt are held.
- Tool Interaction: digit keys select a choice, Tab/Shift+Tab move between
  questions and the Submit step, Enter drives the input/save/advance/submit
  state machine, Esc cancels.
- The help line(s) describe exactly the keys that are routed; navigation hints
  must not advertise keys that do nothing.

### Rendering

- Every surface receives its rect from the frame plan; no surface computes or
  paints outside solved rects.
- Modal surfaces clear their host rect before painting. Decide surfaces fill
  the host with the theme background so the dialog is opaque.
- Paint order is stable: chrome, then plane (unless Browse), then modal layers
  in solve order.

## Pointer-ready contract (geometry only, no input)

The frame plan exposes hit regions derived from its solved rects:

- A hit is `(region id, optional layer index, rect)`; z-order priority is
  layers top-down, then the plane.
- The frame plan answers "what region owns cell (x, y)" as a pure function of
  the solved rects; no separate hand-maintained region table.
- Interactive sub-elements inside a surface (choice rows, tabs, buttons) are
  declared as **sub-region rects relative to the surface rect**, produced by
  the same geometry the renderer uses, so painting and hit-testing cannot
  drift apart.
- Mouse input, hover, and click dispatch are out of scope for this feature and
  will be defined by a future pointer-input PRD on top of this contract.

## Configuration

No user-facing configuration in the initial version. Keybindings keep using
the existing `Keymap` system; pointer input adds no settings.

## Non-goals

- This feature does not implement mouse input.
- It does not allow multiple simultaneous modals or nested dialogs.
- It does not make Decide docks dismissible by clicking outside; the prompt is
  blocking and requires an explicit accept/decline/cancel.

## Acceptance criteria

- [x] With a Decide modal pending, no global shortcut (including F4) changes
      the focused surface; the drawn modal and the focus owner always agree
      (`AppState::modal_surface` + push guard + F4 guard, covered by
      `app/tests/modal_tests.rs`).
- [x] Decide docks are opaque: `InteractiveWorkflow` clears and fills its host
      rect with the theme background before painting.
- [x] Every cell maps to at most one region via the frame plan;
      `FramePlan::hit_test` / `HitMap::hit_test` return the top-most owner
      (engine unit tests with dummy enums + product z-order integration test).
- [x] Surfaces render only into solver-produced rects; no ad-hoc rect math in
      feature renderers.
- [x] Approval and Tool Interaction panels show only routed keys in their help
      lines (approval uses a help override), and letter shortcuts ignore
      Ctrl/Alt-modified events.
- [x] The frame plan's hit-test is covered by unit tests with dummy region
      enums in `piko-tui-layout`, and by an integration test in `piko-tui`
      asserting modal-over-plane z-order.

## Implementation status

Landed:

- `piko-tui-layout`: `Component<E, C>`, `SurfacePanel<R, E, C>`,
  `HitRegion`, `Hit`, `HitMap`, `build_hitmap`, `FramePlan::hit_test`.
- `piko-tui`: modal authority (`AppState::modal_surface` / `pending_decide`),
  Decide push guard, F4 barrier, `InteractionEvent::Requested` defers the
  Tool Interaction surface behind a pending Approval, auto-resolving
  interactions never surface.
- `piko-tui`: `InteractiveWorkflow` completed — arrow choice navigation,
  Tab/Shift+Tab step navigation, truthful state-derived help, approval help
  override, Ctrl/Alt modifier checks, submit/cancel input lock, opaque
  backdrop; `component_regions` exposes tabs/choices/submit as hit data.
- `piko-tui`: `ApprovalPanel` / `ToolInteractionPanel` implement
  `SurfacePanel`; `build_surface_hitmap` builds the per-frame hit map from the
  solved plan.
- `piko-tui`: all remaining surfaces (Agents, Sessions, Tree, SummaryPrompt,
  Status, Diagnostics, Settings, Models, Thinking, AuthSelector, Mcp) now
  implement `Component` / `SurfacePanel`; `render_surface` is a thin match of
  trait calls and every modal surface contributes a surface-default hit
  region.
- `piko-tui`: Decide modals now render through the shared pane chrome
  (`PaneSpec` title/affixes/hint footer + opaque `fill`), with
  `PaneSpec::content_rect` as the single geometry source for both paint and
  hit-testing.

Remaining follow-ups (tracked by the goal):

- Plane regions (Stream, Composer, Notice, Suggest) now have hit specs; the
  pointer-input feature consumes the whole hit map (see
  `pointer-input.md`).
- Drag, hover styling, and per-element actions for Browse/Select surfaces
  remain future work.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Clicks outside the Decide dock | Ignored (no dismiss) | Prompt is blocking; dismissal only via explicit cancel. Revisit in the pointer-input PRD. |
| Decide backdrop | Opaque theme-background fill; dim overlay deferred | Simpler, no double-buffer cost; dim is visual polish, not a contract. |
| Sub-region granularity for hitmaps | Row-level for lists/choices, element-level for tabs/buttons | Enough for click targets; cell-level is overkill for terminal text. |
| Where hit-test geometry lives | Derived from the solved `FramePlan`; sub-regions declared by each surface | Single source of truth; painting and hit-testing cannot drift. |

## Open questions

1. Should Select (ComposerBand) surfaces capture clicks over the composer, or
   be click-through to the underlying editor? (Default: capture — they are a
   focus owner.)
2. Should the backdrop dim be part of the theme system before the pointer-input
   feature lands, or only when hover becomes interactive?
3. Do per-surface hitmaps need stable ids for accessibility/testing, or is a
   rect-only spec sufficient?

## Reference evidence

- Product composition: `packages/tui/src/layout/mod.rs` (`resolve_modal_surface`,
  `compose_frame`).
- Surface catalog and placement policy: `packages/tui/src/navigation/surface.rs`.
- Modal tree composition: `packages/tui/src/navigation/compose.rs`.
- Paint pipeline: `packages/tui/src/render/mod.rs`.
- Key routing and global shortcut bypass: `packages/tui/src/input/focus/router.rs`.
- Shared workflow renderer (no Clear/backdrop today):
  `packages/tui/src/ui/components/interactive_workflow.rs`.
- Engine solver and placement: `packages/tui-layout/src/engine.rs`,
  `packages/tui-layout/src/modal.rs`, `packages/tui-layout/src/focus.rs`.
- Engine contract: `packages/tui-layout/docs/features/shell-flex-layout.md`.
