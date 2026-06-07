# Surface System

Surface decides what extra UI is mounted, where it is mounted, what it covers, and which lower slots should still render.

Interaction behavior belongs to `role` and `focus`. Surface owns mount strategy, layout footprint, z-order, and occlusion. In a TUI, if a lower module is fully covered, it should not render.

## Core model

Use three axes:

1. `mount`: display form and layout position.
2. `role`: interaction contract.
3. `focus`: input ownership.

Occlusion is derived from `mount`, viewport, content size, and resolver policy.

```ts
type SurfaceMount =
  | "replace-slot"
  | "insert-between"
  | "anchored"
  | "side-drawer"
  | "status-line";

type SurfaceSlot =
  | "app"
  | "timeline"
  | "editor"
  | "status"
  | "bottom-bar";

type SurfaceRole =
  | "autocomplete"
  | "selector"
  | "menu"
  | "form"
  | "confirm"
  | "status";

type SurfaceInteractionOwner =
  | "self"
  | "anchor"
  | "none";

interface SurfaceOcclusion {
  covers: SurfaceSlot[];
  fullyCovers: SurfaceSlot[];
}

interface TuiSurfaceState {
  id: string;
  mount: SurfaceMount;
  role: SurfaceRole;
  zIndex: number;
  parentId?: string;
  anchorId?: string;
  targetSlot?: SurfaceSlot;
  insertAfterSlot?: SurfaceSlot;
  occlusion: SurfaceOcclusion;
  interactionOwner: SurfaceInteractionOwner;
  focusOwnerId?: string;
  blocking: boolean;
  data?: unknown;
}
```

Rules:

- `mount` controls display form and layout position.
- `role` controls interaction behavior.
- `occlusion` is computed, not chosen directly by commands.
- `zIndex` controls both visual order and culling order.
- `blocking` controls whether parent input is blocked.
- `interactionOwner` controls where key handling lives.
- `parentId` models nested menus and child forms/confirmations.
- `anchorId` is used for anchored surfaces attached to editor, a row, or a selector item.

## Mount strategies

Mount strategy is the primary surface concept. It describes how the surface appears and how it affects lower slots.

### `replace-slot`

Replaces one or more named slots.

Examples:

- replace `editor` with model selector in compact mode
- replace `timeline` with session tree on narrow terminal
- replace `timeline` with notification history on narrow terminal
- replace app body for setup/login flow when full attention is required

Occlusion:

- The replaced slot is fully covered.
- The replaced slot must not render below the surface.
- If target is `app`, all body slots are fully covered except bottom bar if policy keeps it.

Layout:

- Height equals replaced slot's row budget unless resolver promotes it to app body replacement.
- Stays in the main text flow.
- No centered child window.

### `insert-between`

Inserts a surface into normal layout between slots.

Examples:

- selector between `timeline` and `editor`
- compact confirmation between `status` and `editor`
- latest-output indicator above editor
- login form that stays in text flow

Occlusion:

- Usually covers no existing slot fully.
- May partially cover adjacent slots only if row pressure forces a compact layout policy.
- Lower slots remain rendered unless resolver marks them fully covered.

Layout:

- Consumes rows and triggers layout recompute.
- Best default for TUI panels because it stays in text flow.

This is the important case: "show between editor and status" is `insert-between`, not a new kind.

### `anchored`

Positions a small surface relative to an anchor.

Examples:

- slash autocomplete anchored to editor input
- file autocomplete anchored to cursor/input
- command argument suggestions anchored to selected row

Occlusion:

- Partially covers nearby slots.
- Usually does not fully cover any base slot.
- If it cannot fit, resolver should choose `insert-between`.

Layout:

- Short-lived.
- No long lists.
- Must stay within viewport.
- Does not reserve persistent layout height.

Interaction ownership:

- Anchored does not automatically mean the surface owns focus.
- For slash/file autocomplete, `interactionOwner` should be `"anchor"`.
- The editor remains the active focus owner and receives printable input.
- The attached surface intercepts only navigation/action keys through the anchor owner's interceptors.
- Use `interactionOwner: "self"` only for anchored menus that truly capture focus.
- Use `interactionOwner: "none"` for visual-only anchored hints.

### `side-drawer`

Uses an edge region, usually right or bottom depending on terminal constraints.

Examples:

- session tree on wide terminal
- resume/session browser
- notification history when long

Occlusion:

- On wide terminals, may partially cover timeline while leaving it rendered.
- On narrow terminals, can fully cover timeline or app body.
- Must declare `fullyCovers` after viewport resolution.

Layout:

- Has its own row/column budget.
- Good for long browsing surfaces.

### `status-line`

Mounts into the status slot.

Examples:

- latest notification
- transient command error
- stream/tool progress text

Occlusion:

- Covers the status slot content for that frame.
- Does not cover timeline/editor/bottom-bar.

Layout:

- One line.
- Never captures focus.
- Historical data belongs in notification history, not status line.

## Role capabilities

Role defines behavior, not mount.

| Role | Search | Selection | Hints | Blocking | Typical mounts |
|---|---:|---:|---:|---:|---|
| `autocomplete` | yes | yes | optional | no | `anchored`, `insert-between` |
| `selector` | optional | yes | required | yes | `insert-between`, `replace-slot`, `side-drawer` |
| `menu` | optional | yes | required | usually yes | `insert-between`, `replace-slot`, `anchored` |
| `form` | no | input/action dependent | required | yes | `insert-between`, `replace-slot` |
| `confirm` | no | yes/no | required | yes | `insert-between`, `replace-slot` |
| `status` | no | no | no | no | `status-line` |

## Resolver

Commands return `SurfaceRequest`. They should not choose custom UI shells or occlusion directly.

```ts
interface SurfaceRequest {
  role: SurfaceRole;
  preferredMount?: SurfaceMount;
  targetSlot?: SurfaceSlot;
  contentSize?: "small" | "medium" | "large";
  requiresSecretInput?: boolean;
  destructive?: boolean;
  parentId?: string;
  anchorId?: string;
}

function resolveSurface(request: SurfaceRequest, context: SurfaceContext): TuiSurfaceState;
```

Default policy:

- Autocomplete starts as `anchored` with `interactionOwner: "anchor"`.
- If anchored autocomplete cannot fit, use `insert-between`.
- Small selector starts as `insert-between`.
- Large selector/history/tree uses `side-drawer` on wide terminals.
- On narrow terminals, large selector/history/tree becomes `replace-slot`.
- Login/rename/confirm use blocking `insert-between` first.
- Destructive confirm can become `replace-slot` when it needs unambiguous attention.
- Status uses `status-line`.

## Z-order and culling

TUI z-order should be explicit.

```ts
interface SurfaceLayer {
  surfaceId: string;
  zIndex: number;
  occlusion: SurfaceOcclusion;
}
```

Rendering algorithm:

1. Start with base slots: timeline, status, editor, bottom-bar.
2. Resolve all active surfaces to mount + occlusion.
3. Sort active surfaces by `zIndex`.
4. Compute `fullyCoveredSlots` from highest active surfaces.
5. Do not render base slots in `fullyCoveredSlots`.
6. Do not render lower surfaces fully covered by higher surfaces.
7. Render remaining base slots and surfaces in layout/z-order.

Rules:

- A fully covered lower module should not render.
- `zIndex` is not decorative; it is input to culling and focus.
- A child surface must have higher `zIndex` than parent.
- Closing a parent closes all descendants.
- Focus owner should usually be the highest blocking visible surface.

## Slot relationship

Base app slots:

```text
timeline
status
editor
bottom-bar
```

Common mounts:

- Model selector: `insert-between`, or `replace-slot(editor)` for compact mode.
- Slash autocomplete: `anchored(editor)` with editor-owned interaction.
- Notification latest: `status-line`.
- Notification history: `side-drawer`, or `replace-slot(timeline)` on narrow terminals.
- Login form: `insert-between`, or `replace-slot(editor/timeline)` when setup requires full attention.
- Approval confirm: `insert-between`; destructive approval may use `replace-slot`.

## SurfaceManager

Add:

```text
packages/host-tui/src/surfaces/
  surface-types.ts
  surface-manager.ts
  surface-resolver.ts
  surface-occlusion.ts
```

Renderer hosts:

```text
packages/host-tui/src/renderer/opentui/surfaces/
  SurfaceHost.tsx
  AnchoredSurfaceHost.tsx
  InsertedSurfaceHost.tsx
  DrawerSurfaceHost.tsx
  StatusSurfaceHost.tsx
```

Responsibilities:

- resolve mount from role, viewport, and content size
- derive occlusion from mount and viewport
- assign z-index
- maintain parent-child relationships
- compute fully covered base slots
- close descendants when parent closes
- register related focus owner
- expose visible surfaces to renderer

## Component mapping

| Component | Mount | Notes |
|---|---|---|
| Slash autocomplete | `anchored(editor)` | `interactionOwner: "anchor"`; fallback to `insert-between` if it cannot fit |
| File/path autocomplete | `anchored(editor)` | Same host as slash autocomplete |
| Command argument suggestions | `anchored` | Usually `interactionOwner: "anchor"` |
| Model selector | `insert-between` | `replace-slot(editor)` in compact mode |
| Thinking selector | `insert-between` | Small selector |
| Resume selector | `side-drawer` | `replace-slot` on narrow terminals |
| Settings root/child pages | `insert-between` or `replace-slot(timeline)` | Breadcrumb, no nested borders |
| Login/API key input | `insert-between` | Blocking form, no centered modal |
| Approval prompt | `insert-between` or `replace-slot` | Replace slot for destructive approval |
| Session tree | `side-drawer` | `replace-slot` on narrow terminals |
| Rename session | `insert-between` | Blocking form |
| Status/error | `status-line` | Never steals focus |
| Notifications history | `side-drawer` | `replace-slot` when long/narrow |

## Acceptance criteria

- Surface model is mount-first.
- Mount strategy expresses display form and layout position.
- Occlusion is derived from mount and viewport policy.
- Role is about interaction behavior.
- Fully covered base modules are not rendered.
- `insert-between` supports surfaces between timeline/status/editor without inventing a new kind.
- TUI avoids centered modal child windows.
- `zIndex` affects both rendering and focus ownership.
- Parent close closes child surfaces.
