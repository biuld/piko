# Hint System

Hints must be generated from active keybindings and placed contextually.

## Placement policy

Do not put general hints in `BottomBar`.

| Surface | Hint content |
|---|---|
| Startup/help area | global onboarding: interrupt, clear/exit, commands, bash, expand |
| Slash autocomplete | command names, aliases, descriptions, selected item |
| Selector surfaces | selector-specific navigation and actions |
| Tool/thinking/summary messages | local expand/collapse hints |
| Pending/follow-up queue | queue edit/dequeue hints |
| Status line | transient errors/warnings only |
| Bottom bar | status data only |

pi's footer is status-only: cwd, git branch, session name, token/cache/cost/context usage, model/thinking, extension status line.

## API

```ts
export function keyText(keybinding: KeybindingId): string;
export function keyDisplayText(keybinding: KeybindingId): string;
export function keyHint(keybinding: KeybindingId, description: string): HintNode;
export function rawKeyHint(key: string, description: string): HintNode;
```

`keyHint()` must read from `KeymapManager`, not hardcoded strings.

## Components

```text
packages/host-tui/src/renderer/opentui/hints/
  key-text.ts
  KeyHint.tsx
  HintLine.tsx
  StartupHints.tsx
```

`HintLine` should pack hints to available width and drop low-priority hints first.

## Rules

- Every visible key hint is generated from `KeymapManager`.
- Active surface/focus owner determines visible hints.
- Selector/form/menu hints live in the active surface hint row.
- Inline hints are optional and compact.
- Status line never contains general help text.
- Bottom bar never contains general help text.
