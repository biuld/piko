# Focus System

Focus is a first-class subsystem for editor, timeline, attached autocomplete, nested surfaces, child confirmations, and selector replacement flows.

## Focus regions

```ts
type FocusRegion =
  | "editor"
  | "chat"
  | "surface"
  | "confirm";

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
  match: (event: KeyEvent, state: TuiState) => boolean;
  handle: (event: KeyEvent, state: TuiState) => FocusResult;
}
```

## Required behavior

- Editor owns focus by default.
- Slash/file autocomplete does not steal text focus from editor.
- Attached autocomplete intercepts only navigation/action keys while editor keeps printable input.
- Capturing surfaces push focus and restore previous focus on close.
- Non-interactive status never captures focus.
- Selector replacement flows push focus to selector and restore editor on done.
- Global commands run before owner-specific commands only when explicitly global.
- Owner-specific commands run before editor fallback.
- Owner interceptors run before the owner's fallback key handler.
- Nested menus and nested selectors are first-class focus paths.

## Focus state

```ts
interface TuiFocusState {
  activeOwnerId: string;
  stack: string[];
  region: FocusRegion;
  path: string[];
}
```

Do not rely on `overlay ? "overlay" : "editor"`.

## Attached autocomplete focus

Slash command autocomplete is an attached editor interaction, not an independent focus owner.

The intended behavior:

- Typing `/mo` updates editor text and refreshes autocomplete suggestions.
- `↑` and `↓` move the autocomplete selected index.
- Continuing to type, for example `d`, still goes into editor text.
- `Tab` accepts the selected completion into editor text.
- `Enter` executes the selected/current slash command according to editor submit semantics.
- `Esc` closes autocomplete and keeps editor focused.

This means the active focus owner remains `editor`.

Autocomplete registers as an editor interceptor:

```ts
const slashAutocompleteInterceptor: FocusInterceptor = {
  id: "editor.slash-autocomplete",
  priority: 100,
  match: (_event, state) => state.autocomplete?.active === true,
  handle: (event, state) => {
    if (event.name === "up") return moveAutocomplete(-1);
    if (event.name === "down") return moveAutocomplete(1);
    if (event.name === "tab") return acceptAutocomplete();
    if (event.name === "escape") return closeAutocomplete();
    return { handled: false };
  },
};
```

Printable input should not be routed through the autocomplete interceptor. The editor receives text first, updates its draft, and autocomplete recalculates from the draft.

Attached autocomplete surface:

- Surface mount is usually `anchored(editor)`.
- The surface is interactive, but its interaction is delegated to the editor focus owner.
- It should not push a new focus path entry.
- It should close automatically when editor text no longer matches its provider.
- It should close on submit, cancel, or accepted completion.

## Nested menu focus

Second-level and third-level menus need hierarchical focus ownership.

Examples:

- `/settings` opens settings selector, then `Models` opens a model settings submenu.
- `/session` opens session menu, then `Tree`, `Fork`, `Rename`, or `Delete` opens a child selector/form/confirmation.
- `/model` opens model selector, then a provider row can open provider-scoped model choices.
- `/hotkeys` opens categories, then a category opens a binding editor.
- Command argument completion is normally another editor-attached interceptor. It should become a child focus owner only if it opens a blocking selector or form.

Model this as a focus tree plus active path:

```ts
interface FocusNode {
  id: string;
  region: FocusRegion;
  parentId?: string;
  blocking: boolean;
  restoreTo?: string;
  handleKey?: (event: KeyEvent) => FocusResult;
}

type FocusResult =
  | { handled: true }
  | { handled: false }
  | { push: FocusNode }
  | { pop: true }
  | { popTo: string };
```

Routing rules:

- Send keys to the deepest active focus node first.
- For the active owner, run matching interceptors by priority before the owner's fallback key handler.
- If it returns `handled: false`, bubble to its parent.
- Emergency globals such as exit/interrupt may run before active path only when explicitly marked global.
- `Esc` pops the deepest node by default.
- `Enter` confirms the deepest node, not the parent.
- Closing a parent must close all descendants.
- Closing a child returns focus to parent, not editor.
- Closing the root blocking surface returns to `restoreTo`, usually editor.

## Nested visual policy

- Prefer replacing the current surface body for second-level menus.
- Use breadcrumb text such as `Settings / Models`.
- Use the same hint row, generated from active focus node keybindings.
- Avoid separate bordered boxes for each nested level.
- Use a child blocking surface only for destructive confirmation or credential input.

## App integration

```ts
useKeyboard((key) => {
  focusManager.handleKey(key);
});
```

`App.tsx` should not contain command routing conditionals.
