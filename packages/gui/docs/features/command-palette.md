# GUI Command Palette

> Status: implemented feature contract
> Related: [GUI Overlay Stack](overlay-stack.md),
> [GUI Workbench](workbench.md)
> Design: [GUI Overlay Stack Design](../design/overlay-stack.md)

## Overview

The Command Palette is a Transient overlay that lists hostd command catalog
entries, filters them as the user types, and runs the selected action. It
supports a nested submenu for Models and Thinking level so those defaults are
chosen explicitly instead of cycling from the Composer.

## Layout

```text
┌──────────────────────────────────────┐
│ Command Palette / Models / Thinking  │
│ Search commands…                     │
├──────────────────────────────────────┤
│ Models                            >  │
│ New Session                     /new │
│ …                                    │
└──────────────────────────────────────┘
```

- Centered Transient overlay above the Workbench with a dimmed backdrop.
- Compact island-like panel (surface, 10 px radius): search as the primary
  header on the root; submenu shows a thin crumb title (Models / Thinking).
- Filterable list rows follow Workbench list density (~32 px): title left,
  monospace trailing mark (`/slash` or `>`) right; optional muted detail line.
- Selected and hovered rows use elevated fill (no blue selection border).
- Footer hint for keyboard shortcuts; empty filter shows a muted empty state.

## Behavior and interactions

- `Cmd+Shift+P` opens the palette at the root catalog. Opening is refused while
  a HostPrompt (approval or interaction) is active.
- Typing filters the current frame by title, detail, and trailing label.
- ↑ / ↓ move the selection; Enter confirms:
  - root **Models** → push the model list submenu
  - root **Thinking** → push the thinking-level submenu
  - a model or thinking level → apply via host config and close the palette
  - other runnable commands → run and close the palette
- Escape pops one submenu level; at the root it closes the palette.
- Root rows merge two sources (see
  [Host Command Catalog Design](../../../../docs/host-command-catalog-design.md)): the neutral
  `HostCommandDescriptor` list fetched from hostd, and a small GUI-local list
  (Sessions, Agents, session tree, Settings, Clear notifications, Quit).
  hostd no longer sends slash names, palette-visibility flags, or UI-opener
  actions — the GUI owns that presentation layer.
- `model.set` / `thinking.set` host rows do not run directly; they open the
  Models / Thinking nested picker, same as before.
- Host ids the GUI does not yet have a flow for (rename, import, export,
  delete, fork, clone, compact, login, logout) show as disabled with a
  reason (needs input / needs confirmation / not available in GUI yet).
- `open.settings` is currently a stub notification until the GUI Primary
  Surface Settings view lands.

## Configuration

Default binding: `Cmd+Shift+P`. No persisted `[gui]` key in this wave.

## Non-goals

- Composer `/` slash autocomplete
- `@` file browser
- Full Settings / Help / Login overlay bodies (Models/Thinking live as palette
  submenus only)
- Changing hostd catalog semantics or TUI palette behavior

## Acceptance (user-visible)

- `Cmd+Shift+P` opens a searchable command list over the Workbench.
- Models and Thinking open nested lists; Escape returns to the root before
  closing.
- Selecting a model or thinking level updates Composer labels and closes the
  palette.
- Argument-required and deferred commands do not silently fail.
- The palette does not open over an unresolved approval or interaction.
