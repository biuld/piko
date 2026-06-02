# Focus System

Focus is a first-class subsystem for editor, timeline, autocomplete, nested surfaces, child confirmations, and selector replacement flows.

## Focus regions

```ts
type FocusRegion =
  | "editor"
  | "autocomplete"
  | "chat"
  | "surface"
  | "confirm";

interface FocusOwner {
  id: string;
  region: FocusRegion;
  priority: number;
  handleKey?: (event: KeyEvent) => boolean;
  focus?: () => void;
  blur?: () => void;
}
```

## Required behavior

- Editor owns focus by default.
- Autocomplete temporarily owns input while visible.
- Capturing surfaces push focus and restore previous focus on close.
- Non-interactive status never captures focus.
- Selector replacement flows push focus to selector and restore editor on done.
- Global commands run before owner-specific commands only when explicitly global.
- Owner-specific commands run before editor fallback.
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

## Nested menu focus

Second-level and third-level menus need hierarchical focus ownership.

Examples:

- `/settings` opens settings selector, then `Models` opens a model settings submenu.
- `/session` opens session menu, then `Tree`, `Fork`, `Rename`, or `Delete` opens a child selector/form/confirmation.
- `/model` opens model selector, then a provider row can open provider-scoped model choices.
- `/hotkeys` opens categories, then a category opens a binding editor.
- Command autocomplete opens slash commands, then command argument completion owns a child suggestion list.

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
