# Focus / Interaction Enhancement

This document describes the implemented keyboard interaction architecture in
`host-tui`. It reflects the current code, upgrading the earlier design docs.

It complements:
- [focus.md](focus.md)
- [surface-ux-contract.md](surface-ux-contract.md)
- [autocomplete.md](autocomplete.md)

## Implemented architecture

The keyboard interaction model uses three layers:

```text
OpenTUI KeyboardGateway (App.tsx useKeyboard)
  → normalize key (key-normalize.ts)
  → TuiController.handleKey (runtime/tui-controller.ts)
  → InputRouter.dispatch (focus/input-router.ts)
    → editor child handler (autocomplete, if editor focused)
    → FocusManager.handleKey (global handler → interceptors → owner → bubble)
    → app fallback keymap (if no blocking surface)
```

## Layer responsibilities

### Keyboard Gateway (App.tsx)

The keyboard gateway is always mounted:

```tsx
const { useKeyboard } = useTui();
useKeyboard((event) => controller.handleKey(event));
```

It delegates to `TuiController.handleKey()` and does not inspect command ids,
surface types, or editor state.

### TuiController.handleKey

Normalizes the raw OpenTUI event via `normalizeKeyEvent()` and passes the
normalized event to `InputRouter.dispatch()`. Returns `true` if handled.

### InputRouter

Owns dispatch order:

1. **Editor child handler**: Runs first if set (autocomplete keys). Only
   activates when editor is `FocusManager.isFocused("editor")`.
2. **FocusManager.handleKey**: Full focus routing (global handler → interceptors
   → owner → bubble).
3. **App fallback keymap**: Only when no blocking surface is active. Matches
   keybindings, checks availability, and executes commands.

### FocusManager

Handles the focus tree with:
- **Global handler** (set by TuiController): Esc routing, approval keys,
  double-escape detection.
- **Interceptors** (per owner, by priority): Scoped key handling (e.g.,
  timeline scroll interceptor on editor).
- **Owner key handler**: Per-owner fallback with result processing
  (push/pop/popTo).

### Editor child handler

Set by `Editor` component when autocomplete is active. Handles Up/Down/Tab/
Enter/Esc for autocomplete inline without going through the full focus system.
This is why autocomplete does not need to be a managed surface.

## Esc routing

```text
1. Global handler:
   a. If approval pending: Enter=accept, Esc=decline (block all other keys)
   b. If surface active: close top surface
   c. If autocomplete visible: cancel autocomplete
   d. If stream running: abort stream
   e. Double-escape with empty editor: session tree or fork
2. Fall through if none of the above
```

## Enter routing

```text
1. Surface key controller: confirm/submit if surface active
2. Editor child handler: accept autocomplete if visible
3. Editor submit (normal prompt submission)
```

## Printable input routing

```text
1. Surface filter/form input (if capture-panel active)
2. Editor draft (if editor focused and no blocking surface)
```

If a capture-panel surface is active, `computeRenderPlan()` removes the editor
from the render tree. No printable input can reach the editor structurally,
not just through focus checks.

## Blocking surface guard

`InputRouter` checks `state.surfaces` for any surface with `inputPolicy !== "passive"`.
If a blocking surface exists, the app fallback keymap is suppressed. This
prevents global keybindings from firing underneath a surface.

## Focus state sync

`FocusManager.onChange()` fires on every focus state mutation. `TuiController`
wires this to `store.dispatch({ type: "focus_changed", ... })`. The store
reducer updates `state.focus` which triggers reactive re-renders in any
component reading focus state.

## Surface focus lifecycle

```text
openPanel(request):
  1. surfaceManager.openPanel(request) → surfaceId
  2. focusManager.registerOwner({ id: surfaceId, region: "surface", handleKey: ... })
  3. if inputPolicy !== "passive": focusManager.pushFocus(surfaceId, "surface", "editor")
  4. wire surface key controller → maps keys to PanelRuntime actions

closeSurface(surfaceId):
  1. surfaceManager.close(surfaceId) → emits surface_closed
  2. focusManager.closeSurface(surfaceId) → popToFocus(restoreTarget)
  3. focusManager.unregisterOwner(surfaceId)
```

## Double-escape detection

TuiController tracks the timestamp of the last Escape press. A second Escape
within 500ms with an empty editor triggers the configured action:

- `"tree"` → opens session tree
- `"fork"` → opens session fork
- `"none"` → no action

Configured via `SettingsManager.getDoubleEscapeAction()`.

## Design notes (not yet implemented)

- **InteractionOwner / InteractionKind types**: The current model uses
  `FocusOwner` with a simpler structure. There is no separate `InteractionKind`
  enum or `InteractionManager`.
- **KeyboardCaptureHost**: The keyboard gateway is in `App.tsx` via
  `useKeyboard`. There is no separate capture host component.
- **Modal stacking**: The current model does not distinguish between `blocking`
  and `modal` surfaces. All capture surfaces are treated equally.

## Tests

Test files:
- `packages/host-tui/test/focus-routing.test.ts`
- `packages/host-tui/test/input-router.test.ts`
- `packages/host-tui/test/key-normalize.test.ts`
- `packages/host-tui/test/role-behavior.test.ts`
