# Surface UX Contract

This document defines the current surface interaction contract in `host-tui`.
It reflects the implemented behavior, not aspirational design.

## Implemented model

The surface system uses a simple two-axis model:

- **Placement**: `"partial"` (insert-between) or `"full"` (replace timeline).
- **Input policy**: `"capture"` (blocking) or `"passive"` (non-blocking).

Panel content is modeled through the `panels/` subsystem, which provides
`PanelSession`, `PanelRoute`, `PanelBody`, and `PanelRuntime`. See
[surfaces.md](surfaces.md) for the surface lifecycle and [panels/](../src/panels/types.ts)
for the panel model.

## Key routing contract

All physical key events enter through one route:

```text
OpenTUI useKeyboard
  → normalize key
  → InputRouter.dispatch
    → editor child handler (autocomplete, if editor focused)
    → FocusManager.handleKey (global handler → interceptors → owner → bubble)
    → app fallback keymap (only if no blocking surface)
```

### Esc priority

```text
1. Global handler: close top surface if any active
2. Cancel autocomplete if visible (editor child handler)
3. Interrupt running stream
4. Double-escape with empty editor → session tree / fork
```

### Enter priority

```text
1. Top surface confirm / submit (if surface active)
2. Editor child autocomplete accept (if visible)
3. Editor prompt submit
```

### Printable input

```text
1. Top surface filter / form input (if capture-panel active)
2. Editor draft (if no blocking surface)
```

## Surface lifecycle

1. **Open**: `TuiController.openPanel(request)` creates surface via
   `SurfaceManager.openPanel()`, registers focus owner, pushes focus if
   `inputPolicy !== "passive"`, wires surface key controller.
2. **Key handling**: Surface key controller maps raw keys to panel actions via
   `PanelRuntime.dispatch()`. Default Esc → cancel/close, Enter → confirm/submit.
3. **Close**: `TuiController.closeSurface(id)` removes surface, pops focus,
   unregisters focus owner. Closing a parent closes all descendants.

## Editor availability

When a capture-panel surface (`inputPolicy !== "passive"`) is active:

- The real editor input is removed from the render plan.
- No printable input reaches the editor.
- Slash autocomplete cannot open.
- Prompt submission is blocked.

This is structural (the editor slot is skipped in `computeRenderPlan()`),
not merely a `focused={false}` flag.

## Panel routing

Surfaces embed a `PanelSession` with a route stack:

```ts
interface PanelSession {
  id: string;
  stack: PanelRoute<any>[];   // route stack (push/pop/replace)
  state: PanelState;            // selectedIndex, filterText, formValues
}

interface PanelRoute<TPayload> {
  id: string;
  chrome: PanelChrome;          // title, hints
  interaction: PanelInteraction; // "list" | "menu" | "form" | "confirm" | "passive"
  capabilities: PanelCapability[]; // filter, list, form, detail
  body: PanelBody<TPayload>;     // type + payload
}
```

`PanelRuntime` is the state machine for a panel session. It handles route
navigation (push/pop/replace), filter updates, selection changes, and form
value updates. The runtime calls an `onChange` callback on every mutation and
`onDismiss` when the route stack empties.

## Role behavior

Panel interaction types define default key behavior:

| Interaction | Default keys | Notes |
|---|---|---|
| `list` | Up/Down/PageUp/PageDown, Enter confirm, Esc close | Optional filter via `PanelCapability.filter` |
| `menu` | Up/Down/PageUp/PageDown, Enter activate, Esc close | Optional filter |
| `form` | Text input, Tab/Shift+Tab, Enter submit, Esc cancel | Form values tracked in `panel.state.formValues` |
| `confirm` | Left/Right/Tab, Enter confirm, Esc cancel | Yes/no or destructive confirm |
| `passive` | No key capture | Used for status/read-only panels |

Surface key controllers in `TuiController` map key events to panel actions:

```ts
type SurfaceKeyResult =
  | { type: "handled" }
  | { type: "close" }
  | { type: "confirm"; value?: unknown }
  | { type: "submit"; value?: unknown };
```

## Hints

Hints are derived from panel interaction + keymap. The panel `chrome.hints`
array provides custom hint strings. Default hints:

```text
list:   "↑/↓ navigate  Enter select  Esc close"
menu:   "↑/↓ navigate  Enter activate  Esc close"
form:   "Tab next  Enter submit  Esc cancel"
confirm: "←/→ choose  Enter confirm  Esc cancel"
```

## Design notes (aspirational / not yet implemented)

The following features from earlier design docs are not implemented:

- **SurfaceRole / SurfaceModality**: The current model uses simpler
  `SurfacePlacement` + `SurfaceInputPolicy`. No separate `SurfaceModality`
  (`modal` vs `blocking` vs `nonblocking`).
- **SurfaceResolver**: Mount strategy is chosen by the command/action, not
  resolved automatically from role + viewport.
- **Occlusion computation**: The render plan handles only topmost full/partial
  surface. No general occlusion solver.
- **side-drawer mount**: Not implemented as a true overlay drawer. Large panels
  use `full` placement instead.
- **Stack policy**: Simple topmost-wins for each placement kind. No modal
  stacking above blocking surfaces.
- **Role behavior handlers**: PanelRuntime handles state transitions but
  surface key controllers map keys individually rather than through role-level
  behavior dispatch.
