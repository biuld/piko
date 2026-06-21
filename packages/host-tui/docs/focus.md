# Focus System

Focus is a first-class subsystem for editor, timeline, attached autocomplete,
nested surfaces, child confirmations, and selector replacement flows.

## Implemented model

The focus system consists of three layers:

1. **FocusManager** — focus tree, owner registry, push/pop/popTo, interceptors.
2. **InputRouter** — business key routing pipeline.
3. **Key normalization** — normalizes raw OpenTUI events.

## Focus regions

```ts
type FocusRegion = "editor" | "chat" | "surface" | "confirm";
```

## Focus owners

```ts
interface FocusOwner {
  id: string;
  region: FocusRegion;
  priority: number;
  handleText?: (text: string) => boolean;
  handleKey?: (event: KeyEvent) => FocusResult;
  interceptors?: FocusInterceptor[];
  focus?: () => void;
  blur?: () => void;
}

interface FocusInterceptor {
  id: string;
  priority: number;
  match: (event: KeyEvent, state: any) => boolean;
  handle: (event: KeyEvent, state: any) => FocusResult;
}
```

## Focus results

```ts
type FocusResult =
  | { handled: true }
  | { handled: false }
  | { push: FocusNode }
  | { pop: true }
  | { popTo: string };

interface FocusNode {
  id: string;
  region: FocusRegion;
  parentId?: string;
  blocking: boolean;
  restoreTo?: string;
  handleKey?: (event: KeyEvent) => FocusResult;
}
```

## Focus state

```ts
interface TuiFocusState {
  activeOwnerId: string;
  stack: string[];      // ordered focus stack (deepest last)
  region: FocusRegion;
  path: string[];        // full path including nested owners
}
```

## FocusManager

`FocusManager` (in `src/focus/focus-manager.ts`) owns:

- **Owner registry**: Map of `id → FocusOwner`. Editor is always registered.
- **Focus state**: Active owner, stack, region, path.
- **Push/pop/popTo**: Stack-based focus transitions with restore targets.
- **Global handler**: Emergency keys (Esc interrupt, exit) checked before routing.
- **Interceptor matching**: Scoped key handling per owner with priority ordering.
- **Text handling**: Printable input routed to active owner's `handleText`.
- **Change listener**: Syncs focus state to TUI store on every mutation.

### Key routing algorithm

```text
FocusManager.handleKey(event):
  1. Global handler (Esc, etc.) → if handled, return true
  2. For each owner in stack (deepest first):
     a. If deepest owner, try handleText for printable chars
     b. Run interceptors (sorted by priority)
        → match event against state
        → if handled, process result (push/pop/popTo/handled)
     c. Try owner.handleKey
        → if handled or push/pop/popTo, process and return
        → if not handled, bubble to parent
  3. Return false (not handled)
```

## InputRouter

`InputRouter` (in `src/focus/input-router.ts`) is the single key routing pipeline:

```ts
class InputRouter {
  dispatch(event: KeyEvent): boolean {
    // 1. Editor child handler (autocomplete) — only when editor is focused
    // 2. FocusManager.handleKey — active surface, interceptors, global handler
    // 3. App fallback keymap — only when no blocking surface is active
  }
}
```

The editor child handler is set by `Editor` when autocomplete is active. It
handles Up/Down/Tab/Enter/Esc for autocomplete without going through the full
focus routing. It only activates when editor is the focused owner (not during
surface capture).

## Required behaviors

- Editor owns focus by default.
- Slash/file autocomplete does not steal text focus — it uses the editor child
  handler path in `InputRouter`.
- Capturing surfaces push focus and restore on close (via `focus.pushFocus()` +
  `focus.closeSurface()`).
- Non-interactive status never captures focus.
- Selector replacement flows push focus to selector and restore editor on done.
- Global commands run before owner-specific commands (via `globalHandler`).
- Owner interceptors run before the owner's fallback key handler.
- Nested menus are modeled as stack entries; closing a parent pops all descendants.

## Editor interceptors

The editor focus owner is registered with interceptors:

- **`editor.timeline-scroll`** (priority 50): Intercepts PageUp/PageDown/End
  when no blocking surface is active. Dispatches scroll commands to the
  timeline.

Editor autocomplete is handled as an `editorChildHandler` in `InputRouter`,
not as an interceptor. This ensures it receives keys before the focus system
but only when editor is the active focus owner.

## Surface focus

When a surface is opened with `inputPolicy !== "passive"`:

1. `FocusManager.registerOwner()` adds the surface as a focus owner with
   `region: "surface"` and a key handler wired to the surface key controller.
2. `FocusManager.pushFocus(surfaceId, "surface", "editor")` pushes the surface
   onto the stack with `editor` as the restore target.
3. When the surface closes: `FocusManager.closeSurface(surfaceId)` pops back to
   the restore target.

Surface key controllers map raw keys to `SurfaceKeyResult`:
- `"handled"` → event consumed
- `"close"` → close surface
- `"confirm"` / `"submit"` → run callback + close

## Global handler

The global handler in `FocusManager` (set by `TuiController`) handles:

1. **Approval keys**: Enter to accept, Esc to decline (when approval is pending).
   All other keys blocked.
2. **Esc**: Close top surface → cancel autocomplete → abort stream →
   double-escape tree/fork.
3. Returns `false` for non-handled keys, falling through to normal routing.

## App integration

```tsx
useKeyboard((key) => controller.handleKey(key));
```

`App.tsx` should not contain command routing conditionals. The keyboard gateway
calls `controller.handleKey()`, which normalizes the event and passes it to
`InputRouter.dispatch()`.
