# host-tui Surface and Panel Redesign

## Requirements

1. Managed panels render only inside the variable region above the status line.
2. A managed panel has only two placements:
   - `partial`: occupies part of the variable region.
   - `full`: occupies the entire variable region.
3. `partial` panels draw a bottom border to separate the panel from the status line.
4. Status line, editor, and bottom bar are persistent app regions, not surface placements.
5. Body components must not draw panel border, title, hints, filter row, spacing, or counters.
6. Body components must not receive command-specific layout values such as `maxHeight`.
7. Body components only render payload data and emit domain/panel actions.
8. Panel chrome is data-driven: title, hints, filter capability, and route behavior come from panel route data.
9. Filter rows are panel-frame capability rows, not body component UI.
10. Some panel bodies need keyword filtering.
11. Some panel bodies need two-level or multi-level navigation.
12. Multi-level navigation should usually be route stack changes inside one panel session, not new root surfaces.
13. `Esc` should pop the current route when possible; otherwise it closes the surface.
14. `Enter` may select, submit, toggle, or push a child route depending on the current route.
15. The active route owns title, hints, filter capability, body type, and key behavior.
16. The same panel logic must render as either `partial` or `full` by changing placement only.
17. `full` is not a separate body type and must not require duplicated body components.
18. Surface lifecycle, placement, input policy, dismiss policy, panel route stack, body type, and body payload are separate concepts.
19. A body registry may dispatch body type to body renderer, but it must not own placement, chrome, filter rows, route stack, or panel layout.
20. Surface focus/key routing is based on the active surface and active panel route.
21. Only one root surface with captured input should own keyboard input at a time unless an explicit policy says otherwise.
22. Autocomplete remains editor-owned and is not a managed panel surface.
23. App-level confirm/approval flows use `full` placement unless they require a separate non-panel runtime.
24. Legacy global drawer, anchored, status-line surface, arbitrary slot replacement, and occlusion models should be removed.

## Design Summary

Use three small concepts:

```txt
Surface = lifecycle + placement + input/dismiss policy + z-order
PanelSession = route stack + panel-local state
PanelRoute = chrome + capabilities + interaction + body
```

The renderer then becomes a simple mapping:

```txt
Surface.placement -> PartialPanelHost | FullPanelHost
PanelRoute        -> PanelFrame
PanelBody.type    -> PanelBodyRegistry
```

There is no separate selector/menu/form surface class. Interaction is a route property. There is no slot/occlusion resolver. Placement is only `partial` or `full`.

## Core Types

### Surface

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
}
```

Defaults for command panels:

```ts
inputPolicy: "capture";
dismissPolicy: "route-pop-or-close";
```

### Panel Session

```ts
interface PanelSession {
  id: string;
  stack: PanelRoute[];
  state: PanelState;
}

interface PanelState {
  selectedIndex?: number;
  filterText?: string;
  formValues?: Record<string, string>;
}
```

The session survives placement changes. Moving Settings from `partial` to `full` changes `surface.placement`, not the session or body.

### Panel Route

```ts
interface PanelRoute<TPayload = unknown> {
  id: string;
  chrome: PanelChrome;
  interaction: PanelInteraction;
  capabilities: PanelCapability[];
  body: PanelBody<TPayload>;
}

interface PanelChrome {
  title: string;
  hints?: string[];
}
```

Current route determines title, hints, optional filter row, body type, and key behavior.

### Panel Capabilities

```ts
type PanelCapability =
  | { kind: "filter"; placeholder?: string }
  | { kind: "list"; selectable: true }
  | { kind: "form" }
  | { kind: "detail" };
```

`filter` means `PanelFrame` renders a filter row and panel runtime stores `filterText`. Body renderers consume `filterText`; they do not draw input UI.

### Panel Interaction

```ts
type PanelInteraction =
  | "list"
  | "menu"
  | "form"
  | "confirm"
  | "passive";
```

Interaction belongs to the active route, not the surface.

### Panel Body

```ts
type PanelBodyType =
  | "model-picker"
  | "thinking-picker"
  | "session-resume"
  | "settings"
  | "login"
  | "notifications"
  | "hotkeys"
  | "help"
  | "changelog"
  | "session-info"
  | "session-tree"
  | "session-fork"
  | "session-import"
  | "session-rename";

interface PanelBody<TPayload = unknown> {
  type: PanelBodyType;
  payload: TPayload;
}
```

Body components receive payload and panel context only.

## Layout Model

The app has persistent regions:

```txt
variable region above status
status line
editor
bottom bar
```

Managed surfaces render only in the variable region:

```txt
full:
  panel fills the whole variable region

partial:
  timeline/content remains above
  panel occupies lower slice of variable region
  panel draws bottom border before status
```

The host computes layout metrics:

```ts
interface PanelMetrics {
  width: number;
  height: number;
  bodyRows: number;
  density: "compact" | "comfortable";
  placement: SurfacePlacement;
}
```

Only placement hosts compute these metrics.

## Rendering Model

```txt
AppShell
  VariableRegion
    TimelineRegion
    SurfaceLayer
      SurfaceHost
        PartialPanelHost | FullPanelHost
          PanelFrame
            header
            optional filter row
            body viewport
            hints/footer
            PanelBodyRegistry
  StatusLine
  Editor
  BottomBar
```

### PanelFrame

`PanelFrame` owns:

- title/header;
- filter row;
- body viewport;
- footer/hints;
- spacing;
- border rules;
- body row budget.

### PanelBodyRegistry

`PanelBodyRegistry` only maps `route.body.type` to a renderer.

It must not:

- generate title or hints;
- decide placement;
- draw filter rows;
- own route stack;
- compute max height;
- know whether the host is `partial` or `full`.

## Input Model

Input routing:

```txt
raw key
  -> normalize
  -> InputRouter
  -> active captured surface
  -> PanelRuntime current route
  -> optional body action
```

Rules:

- `Esc` with `route-pop-or-close`: pop route if stack depth > 1, else close surface.
- `Esc` with `manual`: route/body logic decides.
- printable input with filter capability: update `panel.state.filterText`.
- printable input with form interaction: update form state.
- arrows/page/home/end with list/menu interaction: update selection.
- `Enter`: dispatch route-specific action.

## Data Examples

### Settings Partial Panel

```ts
openSurface({
  placement: "partial",
  inputPolicy: "capture",
  dismissPolicy: "route-pop-or-close",
  panel: {
    id: "settings",
    stack: [
      {
        id: "settings.root",
        chrome: {
          title: "Settings",
          hints: ["Up/Down move  Enter change  Esc close"],
        },
        interaction: "menu",
        capabilities: [{ kind: "list", selectable: true }],
        body: {
          type: "settings",
          payload: {},
        },
      },
    ],
    state: { selectedIndex: 0 },
  },
});
```

### Resume Partial Panel With Filter

```ts
openSurface({
  placement: "partial",
  inputPolicy: "capture",
  dismissPolicy: "route-pop-or-close",
  panel: {
    id: "resume",
    stack: [
      {
        id: "resume.list",
        chrome: {
          title: "Resume Session",
          hints: ["Type filter  Up/Down move  Enter open  Esc close"],
        },
        interaction: "list",
        capabilities: [
          { kind: "filter", placeholder: "Filter sessions..." },
          { kind: "list", selectable: true },
        ],
        body: {
          type: "session-resume",
          payload: { scope: "all" },
        },
      },
    ],
    state: { filterText: "", selectedIndex: 0 },
  },
});
```

### Same Settings Panel As Full

```ts
surfaceManager.updatePlacement(surfaceId, "full");
```

or:

```ts
openSurface({
  placement: "full",
  inputPolicy: "capture",
  dismissPolicy: "route-pop-or-close",
  panel: createSettingsPanelSession(),
});
```

## Current Panel Mapping

| Current type | Placement | Interaction | Capabilities |
|---|---|---|---|
| `model` | `partial` | `list` | `filter`, `list` |
| `thinking` | `partial` | `list` | `list` |
| `resume` | `partial` | `list` | `filter`, `list` |
| `settings` | `partial` | `menu` | `list` |
| `login` | `partial` | `form` | `form` |
| `notifications` | `partial` | `list` | `list` |
| `hotkeys` | `partial` | `list` | `filter`, `list` |
| `help` | `partial` | `list` | `filter`, `list` |
| `changelog` | `partial` | `passive` | `detail` |
| `session-info` | `partial` | `passive` | `detail` |
| `fork-session` | `partial` | `list` | `list` |
| `session-tree` | `partial` | `list` | `filter`, `list` |
| `import-session` | `partial` | `form` | `form` |
| `rename-session` | `partial` | `form` | `form` |
| destructive confirm | `full` | `confirm` | `detail` |

## Deletions And Simplifications

Remove these concepts from the target model:

- `SurfaceMount`
- `SurfaceSlot`
- `SurfaceRole`
- `SurfaceModality`
- `SurfaceInteractionOwner`
- `contentSize`
- `targetSlot`
- `insertAfterSlot`
- `anchorId`
- `parentId` for normal panel navigation
- `focusOwnerId`
- `occlusion`
- slot replacement logic
- arbitrary insert-between layout logic
- drawer host
- anchored global command surface
- status-line surface placement
- registry-level chrome fallback
- body-level `maxHeight`
- body-level scroll counter decisions

After deletion, `SurfaceState` should be close to:

```ts
interface SurfaceState {
  id: string;
  placement: "partial" | "full";
  inputPolicy: "capture" | "passive";
  dismissPolicy: "route-pop-or-close" | "manual";
  zIndex: number;
  panel: PanelSession;
}
```

## Target Directory Shape

```txt
packages/host-tui/src/
  surfaces/
    types.ts
    surface-manager.ts

  panels/
    types.ts
    panel-runtime.ts
    panel-actions.ts
    panel-factories.ts
    body-registry.ts

  renderer/opentui/panels/
    PanelFrame.tsx
    PartialPanelHost.tsx
    FullPanelHost.tsx
    PanelBodyRegistry.tsx
    bodies/
      SettingsBody.tsx
      ModelPickerBody.tsx
      ResumeBody.tsx
```

Generic body widgets, such as `SelectListView`, can stay under a shared renderer widgets directory, but they should read generic `PanelMetricsContext`, not editor-panel-specific constants.

## Migration Plan

### Phase 1: Add New Types Alongside Old Types

- Add `SurfacePlacement = "partial" | "full"`.
- Add `SurfaceInputPolicy`.
- Add `SurfaceDismissPolicy`.
- Add `PanelSession`, `PanelRoute`, `PanelCapability`, `PanelInteraction`, `PanelBody`, and `PanelState`.

### Phase 2: Add Panel Factories

- Create panel factories such as `createSettingsPanelSession()`, `createResumePanelSession()`, and `createModelPickerPanelSession()`.
- Commands call panel factories instead of passing loose `data.type`, `contentSize`, or role fields.

### Phase 3: Add Panel Runtime

- Implement route stack operations: `pushRoute`, `popRoute`, `replaceRoute`.
- Implement selection, filter, and form state updates in panel runtime.
- Move route keyboard behavior out of body components.

### Phase 4: Replace Registry

- Rename/split `SurfaceContentRegistry` into `PanelBodyRegistry`.
- Delete registry-level chrome resolution.
- Body registry receives current route body only.

### Phase 5: Replace Placement Hosts

- Replace inserted/replaced slot hosts with `PartialPanelHost` and `FullPanelHost`.
- Keep both hosts inside the variable region above status.
- Draw the partial bottom border in `PartialPanelHost`.

### Phase 6: Simplify Render Plan

- Remove slot replacement and occlusion logic.
- Render variable region as:
  - no panel: timeline fills region;
  - partial panel: timeline plus partial panel;
  - full panel: panel fills region.

### Phase 7: Normalize Body Components

- Rename concrete command components to bodies.
- Remove panel height, title, hints, filter row, border, and counter logic from bodies.
- Bodies consume payload and panel context only.

### Phase 8: Delete Legacy Types And Tests

- Delete old slot/mount/role/modality tests.
- Replace with tests for:
  - partial/full render planning;
  - panel route push/pop;
  - filter capability;
  - input/dismiss policy;
  - body registry dispatch;
  - placement switch preserving panel session.

## Target Invariant

```txt
Command creates a PanelSession.
Surface owns lifecycle, placement, input policy, dismiss policy, and z-order.
Placement host owns geometry inside the variable region above status.
PanelFrame owns title, filter row, body viewport, hints, spacing, and borders.
PanelRuntime owns route stack and panel-local state.
PanelBodyRegistry dispatches by body type.
Body component renders payload only.
```

Changing a panel from partial to full must not change panel route data, body type, body component, filter state, selected index, or form state.
