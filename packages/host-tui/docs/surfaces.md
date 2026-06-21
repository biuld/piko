# Surface System

Surface manages panel surface lifecycle, z-order, and render plan computation.
It decides what extra UI is mounted, whether it replaces or inserts between
base slots, and which base slots should still render.

## Architecture

Surfaces use a two-axis model:

1. **Placement**: display form — `"partial"` (insert-between) or `"full"` (replace timeline).
2. **Input policy**: interaction behavior — `"capture"` (blocking) or `"passive"` (non-blocking).

Surface content is modeled through the `panels/` subsystem (`PanelSession`,
`PanelRoute`, `PanelBody`). The surface layer does not care about panel
internals — it only manages lifecycle (open/close), z-order, and render plan.

## Core types

```ts
type SurfacePlacement = "partial" | "full";

type SurfaceInputPolicy = "capture" | "passive";

type SurfaceDismissPolicy = "route-pop-or-close" | "manual";

interface SurfaceState {
  id: string;
  placement: SurfacePlacement;
  inputPolicy: SurfaceInputPolicy;
  dismissPolicy: SurfaceDismissPolicy;
  zIndex: number;
  panel: PanelSession;
  parentId?: string;
}
```

- `placement`: `"full"` replaces the timeline slot. `"partial"` inserts between status and editor.
- `inputPolicy`: `"capture"` blocks key routing to editor. `"passive"` allows editor input.
- `dismissPolicy`: `"route-pop-or-close"` auto-closes on empty route stack. `"manual"` requires explicit close.
- `zIndex`: Controls visual order and culling order. Assigned incrementally on open.
- `panel`: References the `PanelSession` from the `panels/` subsystem.

## SurfaceManager

`SurfaceManager` (in `src/surfaces/surface-manager.ts`) is the lifecycle owner:

```ts
class SurfaceManager {
  openPanel(request: PanelSurfaceRequest): string;
  close(id?: string): void;
  getAllSurfaces(): SurfaceState[];
  getSurface(id: string): SurfaceState | undefined;
  getContext(viewportWidth, viewportHeight, hasActiveStream): SurfaceContext;
  onEvent(fn: (event: SurfaceEvent) => void): () => void;
}
```

Events:

```ts
type SurfaceEvent =
  | { type: "surface_opened"; surface: SurfaceState }
  | { type: "surface_closed"; surfaceId: string };
```

Closing a surface also closes all descendants (surfaces with matching `parentId`).

`TuiController` listens to these events and dispatches corresponding store events
(`surface_opened`, `surface_closed`) and registers/unregisters focus owners.

## Placement strategies

### `full`

Replaces the timeline slot entirely. The surface takes all remaining vertical
space. Used for large selectors, session resume, notification history, and
session tree on narrow terminals.

Examples:
- Session tree when viewport is narrow
- Notification history when long
- Login/setup when full attention is required

### `partial`

Inserts between status and editor in the normal layout flow. Used for smaller
selectors, forms, and confirmations.

Examples:
- Model selector
- Thinking selector
- Settings page
- Session rename form
- Approval prompt

## Render plan

`computeRenderPlan()` (in `src/surfaces/render-plan.ts`) computes the ordered
render entries from current state:

```ts
interface RenderPlanEntry {
  kind: "slot" | "surface";
  id: string;
  placement?: "partial" | "full";
  surface?: SurfaceState;
}

function computeRenderPlan(state: TuiState): { inline: RenderPlanEntry[] };
```

Order:

1. **Timeline / full panel**: If a `full` panel is active, it replaces the
   timeline slot. Otherwise, the timeline renders normally.
2. **Status**: Renders only when no capture-panel surface is active.
3. **Partial panel**: Inserts after status, before editor.
4. **Editor**: Renders only when no capture-panel surface is active.
5. **Bottom bar**: Always renders.

When a capture-panel surface (`inputPolicy !== "passive"`) is active, both
status and editor are hidden from the render plan. The panel owns all remaining
vertical space below the timeline.

Only the topmost surface of each placement kind is rendered (highest zIndex
wins for `full`, highest zIndex wins for `partial`).

## Panel integration

Surfaces reference `PanelSession` from the `panels/` subsystem. The panel
session owns the route stack, state, and capabilities. The surface layer wraps
the panel session with placement, input policy, and z-order.

When a surface is opened via `TuiController.openPanel()`, the controller:

1. Calls `SurfaceManager.openPanel()` to create the surface state.
2. Registers a focus owner for the surface.
3. If `inputPolicy !== "passive"`, pushes focus to the surface.
4. Wires a surface key controller that dispatches panel actions.

## Focus integration

Surfaces with `inputPolicy: "capture"` register as focus owners with `region: "surface"`.
The focus owner receives key events first (before editor). Closing the surface
restores focus to the previous owner (typically editor).

Surfaces with `inputPolicy: "passive"` do not capture focus and do not block
editor input.

## Component mapping

| Command | Placement | Input Policy | Notes |
|---|---|---|---|
| `/model` (model selector) | `partial` | `capture` | Blocks editor |
| `/thinking` (thinking selector) | `partial` | `capture` | Blocks editor |
| `/settings` (settings) | `partial` | `capture` | Blocks editor; has nested routes |
| `/resume` (session resume) | `full` | `capture` | Replaces timeline on all viewports |
| `/tree` (session tree) | `full` / `partial` | `capture` | Full on narrow, partial on wide |
| `/notifications` | `full` | `capture` | Replaces timeline |
| `/login` | `partial` | `capture` | Blocking form |
| `/fork` | `partial` | `capture` | Confirmation panel |
| `/rename` (session rename) | `partial` | `capture` | Blocking form |
| Tool approval | `partial` | `capture` | Blocks editor until resolved |

## Z-order and culling

- `zIndex` is assigned incrementally on open (previous max + 10).
- A child surface has higher `zIndex` than its parent.
- Closing a parent closes all descendants.
- Only the topmost `full` and topmost `partial` surface render.
- `computeRenderPlan()` determines which base slots render based on the
  presence of capture-panel surfaces.

## Key routing with surfaces

When a capture-panel surface is active:

- Key events route to the surface's focus owner first.
- The surface key controller maps raw keys to panel actions (navigate, filter,
  confirm, cancel).
- Global keys (Esc for interrupt, etc.) are checked before surface routing.
- If no blocking surface is active, keys fall through to the app fallback keymap.
