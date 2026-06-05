# host-tui Editor and Autocomplete Unification

This optimization plan moves autocomplete into the editor boundary and removes
it from the managed surface model.

It depends on the focus/interaction enhancement defined in:

- [../packages/host-tui/docs/focus-interaction-enhancement.md](../packages/host-tui/docs/focus-interaction-enhancement.md)
- [../packages/host-tui/docs/surface-ux-contract.md](../packages/host-tui/docs/surface-ux-contract.md)

## Problem

Autocomplete currently behaves like an editor feature but has repeatedly been
treated as surface-like UI. That creates ambiguous ownership:

- printable input belongs to editor
- Up/Down/Tab often belong to autocomplete
- Enter may execute a selected slash command or submit editor text
- Esc may close autocomplete, close a command surface, or abort a stream
- blocking command surfaces must suppress editor autocomplete entirely

If autocomplete is modeled as a managed surface, it competes with global command
panels such as `/settings`, `/resume`, and `/model`. That is the wrong boundary.

## Decision

Autocomplete is part of editor.

```text
Editor
  ├── EditorInput
  ├── EditorAutocompleteController
  └── EditorAutocompleteView
```

Autocomplete can still render visually like a panel above the input, but its
lifecycle, state, and keyboard behavior belong to editor.

It should not be opened through `SurfaceManager`.

## Architecture Boundary

### Editor-Owned

Editor owns:

- draft
- cursor
- local submit behavior
- slash autocomplete state
- file autocomplete state
- command argument autocomplete state
- autocomplete selection index
- autocomplete accept/cancel behavior

### Surface-Owned

Managed surfaces own:

- `/settings`
- `/resume`
- `/model`
- `/thinking`
- `/help`
- `/login`
- `/notifications`
- forms, selectors, menus, confirmations opened by commands

### Focus-Owned

The focus/interaction subsystem owns:

- physical key routing
- active owner stack
- editor child interaction routing
- managed surface routing
- Esc/Enter/Tab priority
- bubbling and restore behavior

## Interaction Model

No managed surface active:

```text
stack = [app, editor]
```

Slash autocomplete visible:

```text
stack = [app, editor, editor.autocomplete]
```

Settings surface open:

```text
stack = [app, editor, surface.settings]
```

Settings edit mode:

```text
stack = [app, editor, surface.settings, surface.settings.editField]
```

Command palette over settings, if needed:

```text
stack = [app, editor, surface.settings, modal.commandPalette]
```

The important distinction:

- `editor.autocomplete` is an editor child interaction
- `surface.settings` is a managed blocking surface
- `modal.commandPalette` has its own input and is not the editor

## Keyboard Rules

### Printable Input

```text
if managed blocking/modal surface active:
  printable input goes to top surface/form/filter
else:
  printable input goes to editor draft
```

Autocomplete does not own printable input. It reacts to editor draft changes.

### Up / Down

```text
if editor.autocomplete visible:
  move autocomplete selection
else if surface active:
  move surface selection
else:
  editor or timeline behavior
```

### Tab

```text
if editor.autocomplete visible:
  accept selected completion
else if form surface active:
  move field
else:
  editor tab behavior
```

### Enter

```text
if surface active:
  confirm/submit surface
else if editor.autocomplete visible and command selection is active:
  accept/execute selected command
else:
  submit editor draft
```

### Esc

```text
if surface/modal active:
  close or cancel top interaction
else if editor.autocomplete visible:
  close autocomplete
else if stream running:
  abort
else:
  no-op
```

This order prevents invisible autocomplete state from blocking surface close.

## Component Structure

Target renderer layout:

```text
renderer/opentui/editor/
  Editor.tsx
  EditorInput.tsx
  EditorAutocompleteView.tsx
```

Domain/editor logic:

```text
src/editor/
  editor-state.ts
  editor-autocomplete-controller.ts
  editor-autocomplete-state.ts
  editor-actions.ts
```

Autocomplete providers remain separate:

```text
src/autocomplete/
  combined-provider.ts
  slash-provider.ts
  file-provider.ts
  types.ts
```

`Editor.tsx` composes these pieces but does not become a large state machine.

## Runtime Rules

1. Editor renders autocomplete only inside editor layout.
2. Autocomplete does not create a `TuiSurfaceState`.
3. Autocomplete does not register with `SurfaceManager`.
4. Autocomplete registers as an editor child interaction only when visible.
5. Blocking/modal surfaces prevent editor from receiving keys.
6. Blocking/modal surfaces may remove the real editor input from the render tree.
7. A stable keyboard gateway remains mounted independent of editor input.

## Current Code Migration

### Phase 1: Clarify Ownership

- Keep `EditorAutocompleteController`.
- Keep `CommandAutocomplete` as a visual component.
- Rename or move `CommandAutocomplete` to `EditorAutocompleteView`.
- Ensure no autocomplete state is stored in global TUI state.
- Ensure `SurfaceRole` does not include autocomplete.

### Phase 2: Route Through Interaction Manager

- Add `editor.autocomplete` as an editor child owner.
- It handles Up/Down/Tab/Esc/Enter.
- It returns semantic key results instead of directly triggering global side
  effects.
- Editor applies accepted completion to draft.
- Controller executes slash commands only after editor submit semantics choose a
  command.

### Phase 3: Remove Surface Coupling

- Delete any autocomplete-specific surface events.
- Delete autocomplete role handling from `SurfaceResolver`.
- Delete autocomplete surface tests.
- Replace with editor autocomplete interaction tests.

### Phase 4: Stabilize Blocking Surface Behavior

- When managed blocking/modal surface is active, editor input is not mounted or
  is not in the interaction route.
- Slash autocomplete cannot open while a blocking surface is active.
- `/settings` followed by typing `/resume` through editor is impossible.
- A modal command palette, if desired, must be a separate surface with its own
  input.

## Tests

Required tests:

- typing `/` with editor active shows autocomplete immediately
- typing more characters updates editor draft and autocomplete items
- Up/Down move autocomplete selection
- Tab accepts completion into editor draft
- Enter executes selected slash command only when command autocomplete owns the
  submit decision
- Esc closes autocomplete when no managed surface exists
- Esc closes top managed surface before autocomplete
- opening `/settings` disables editor autocomplete
- closing `/settings` restores editor and permits autocomplete again

## Anti-Patterns

Avoid:

- opening autocomplete with `openSurface`
- making autocomplete a `SurfaceRole`
- giving autocomplete independent blocking/focus ownership
- letting autocomplete consume Esc while a managed surface is open
- using editor input as the only global keyboard capture host
- letting command panels and editor autocomplete share the same lifecycle stack

## Acceptance Criteria

This optimization is complete when:

- autocomplete is entirely editor-owned
- `SurfaceManager` never sees autocomplete
- focus/interaction stack models autocomplete as an editor child owner
- managed surfaces always win over editor autocomplete for Esc/Enter
- blocking surfaces prevent editor input and autocomplete from participating
- tests cover editor autocomplete and managed surface priority

