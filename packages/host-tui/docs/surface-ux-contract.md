# Surface UX Contract

This document defines the target contract for command panels, global surfaces,
editor-local autocomplete, focus, and keyboard routing in `host-tui`.

The goal is to stop treating each command panel as a special case. Commands
declare what surface they need. Surface runtime decides placement, stack policy,
focus ownership, keyboard behavior, and editor availability.

## Problem

The current implementation has grown several overlapping responsibilities:

- Commands can still influence surface layout through request fields or local
  component choices.
- Components handle `Esc`, `Enter`, filtering, and closing independently.
- Editor-local autocomplete and global command panels are both described as
  "surface-like" interactions, but they have different ownership models.
- Blocking panels can coexist with editor input unless the renderer explicitly
  disables or removes the editor.
- `side-drawer` is present as a mount strategy, but the current renderer does
  not provide a true overlay drawer, so wide panels can appear in surprising
  places.

These are architecture issues, not individual `/settings` or `/resume` bugs.

## Non-Negotiable Rules

1. Commands bind to surface intent, not layout.
2. `SurfaceRole` defines default keyboard semantics.
3. `SurfaceModality` defines whether the editor is interactive.
4. `SurfaceResolver` is the only place that maps intent to mount/slot.
5. `TuiController` is the only global key router.
6. Blocking surfaces remove the real editor input from the render tree.
7. Editor-local autocomplete is not a global surface.
8. Components provide content and callbacks; they do not invent close/focus
   semantics.

## Surface Kinds

Use two separate concepts: editor-local interactions and managed surfaces.

### Editor-Local Interactions

Examples:

- slash command autocomplete
- file autocomplete
- command argument hints that stay attached to the draft

Rules:

- Owned by the editor.
- Do not enter `SurfaceManager`.
- Do not push focus.
- Do not block global panels.
- Close when the draft no longer matches the provider.
- Handle only editor-local keys: up/down/tab/enter/escape while autocomplete is
  visible.

### Managed Surfaces

Examples:

- `/settings`
- `/resume`
- `/model`
- `/thinking`
- `/help`
- `/login`
- `/fork`
- confirmation panels
- approval panels

Rules:

- Enter `SurfaceManager`.
- Receive an id, z-index, modality, mount, occlusion, and focus owner.
- Are routed through `TuiController`.
- May block the editor depending on modality.

## Core Types

Target shape:

```ts
type SurfaceRole =
  | "selector"
  | "menu"
  | "form"
  | "confirm"
  | "status";

type SurfaceModality =
  | "nonblocking"
  | "blocking"
  | "modal";

type SurfaceMount =
  | "insert-between"
  | "replace-slot"
  | "status-line"
  | "anchored"
  | "side-drawer";

interface SurfaceRequest {
  role: SurfaceRole;
  modality?: SurfaceModality;
  contentSize?: "small" | "medium" | "large";
  destructive?: boolean;
  parentId?: string;
  anchorId?: string;
  data: {
    type: string;
    [key: string]: unknown;
  };
}
```

Command code must not set:

```ts
preferredMount
targetSlot
insertAfterSlot
occlusion
blocking
interactionOwner
zIndex
```

Those are runtime decisions.

## Command Binding

Commands directly bind to the surface they need:

```ts
// /settings
{
  role: "menu",
  modality: "blocking",
  contentSize: "medium",
  data: { type: "settings" },
}

// /resume
{
  role: "selector",
  modality: "blocking",
  contentSize: "large",
  data: { type: "resume", filter: args },
}

// /login
{
  role: "form",
  modality: "modal",
  contentSize: "small",
  data: { type: "login", provider },
}
```

The command does not decide whether `/resume` is a drawer, replace-slot panel,
or inserted panel. The resolver decides that from role, modality, content size,
viewport, and current stack.

## Role Keyboard Contracts

`SurfaceRole` defines default key behavior. `data.type` defines business
behavior.

| Role | Default keys | Text input | Default close |
|---|---|---:|---|
| `selector` | Up/Down/PageUp/PageDown, Enter confirm, Esc close | optional filter | Esc |
| `menu` | Up/Down/PageUp/PageDown, Enter activate, Esc close | optional filter | Esc |
| `form` | text input, Tab next field, Shift+Tab previous, Enter submit, Esc cancel | yes | Esc |
| `confirm` | Left/Right/Tab switch, Enter confirm, Esc cancel | no | Esc |
| `status` | none | no | none |

Role behavior returns a semantic result:

```ts
type SurfaceKeyResult =
  | { type: "handled" }
  | { type: "close" }
  | { type: "confirm"; value?: unknown }
  | { type: "submit"; value?: unknown }
  | { type: "unhandled" };
```

`TuiController` interprets the result:

```ts
switch (result.type) {
  case "close":
    closeSurface(surface.id);
    break;
  case "confirm":
    runSurfaceAction(surface, result.value);
    break;
  case "submit":
    submitSurfaceForm(surface, result.value);
    break;
}
```

Components should not call `closeSurface` directly for default Esc behavior.
They may request `close` through the role behavior result.

## Component Responsibilities

Components provide content, not global interaction rules.

Allowed component responsibilities:

- render rows, fields, labels, and values
- provide list items
- provide form field definitions
- expose confirm/submit callbacks
- maintain narrow local UI state such as current text field value
- provide business-specific validation

Disallowed component responsibilities:

- choosing mount strategy
- deciding whether editor is blocked
- deciding z-index
- deciding whether Esc closes the global surface
- opening another blocking command surface from editor while a blocking surface
  is active
- hardcoding key hints that conflict with role/keymap behavior

Exception: local nested state may consume Esc before close. Example: settings
edit mode may use Esc to exit edit mode. If not in edit mode, Esc falls back to
the role default and closes the surface.

## Modality Rules

### `nonblocking`

Use for status and passive visual surfaces.

Rules:

- Does not capture focus.
- Does not remove editor input.
- Does not block prompt submission.
- Usually uses `status-line` or visual-only `anchored`.

### `blocking`

Use for normal command panels.

Rules:

- Captures focus.
- Removes real editor input from the render tree.
- Restores editor when closed.
- Top blocking surface receives keys first.
- Opening another root blocking surface replaces or rejects the current one
  according to stack policy.

### `modal`

Use for approval, destructive confirm, credential input, and app-level setup.

Rules:

- Captures focus above blocking surfaces.
- May cover app body.
- Must be closed explicitly through cancel/submit/confirm.
- Esc behavior depends on role and risk. Destructive irreversible actions may
  require explicit selection instead of implicit Esc confirm.

## Layout Rules

Until `side-drawer` is implemented as a true overlay, do not use it for command
panels.

Default resolver policy:

| Request | Mount | Slot |
|---|---|---|
| `status` + `nonblocking` | `status-line` | `status` |
| `selector/menu/form/confirm` + small/medium + `blocking` | `insert-between` | after `status`, before editor |
| `selector/menu` + large + `blocking` | `replace-slot` | `timeline` |
| `form` + `modal` | `replace-slot` or `insert-between` by viewport | `timeline` or `app` |
| destructive `confirm` + `modal` | `replace-slot` | `app` |

`replace-slot` surfaces render at the replaced slot's position. They must not be
appended after editor.

`insert-between` command panels should use a single insertion point by default:
after status and before editor. Commands should not choose insertion points.

## Stack Policy

The surface stack must be deterministic.

Recommended root opening policy:

| Existing top | New request | Policy |
|---|---|---|
| none | blocking | open |
| blocking | blocking | replace top root surface |
| blocking | modal | push modal |
| modal | blocking | reject or queue |
| modal | modal child | push child modal |
| nonblocking | blocking | open blocking above nonblocking |

Nested surfaces must declare `parentId`. Closing a parent closes descendants.

For command palette workflows, replacing top blocking surface is usually better
than stacking. Example: if `/settings` is open and a command somehow requests
`/resume`, the runtime should replace settings with resume or reject the request
with a notification. It must not let editor open resume underneath/over settings.

## Key Routing Order

All physical key events enter through one route:

```text
OpenTUI useKeyboard
-> normalize key
-> TuiController.handleKey
-> top managed surface if any
-> editor-local autocomplete if editor is active
-> editor input/keymap
-> app fallback keymap
```

`Esc` order:

```text
top modal/blocking surface
-> editor-local autocomplete
-> stream abort
-> no-op
```

`Enter` order:

```text
top surface confirm/submit if surface active
-> editor-local autocomplete accept/execute if visible
-> editor submit
```

Printable input order:

```text
top form/menu/selector filter if surface active
-> editor draft if no blocking/modal surface
```

## Editor Availability

When a blocking or modal surface is active:

- Do not render the real editor input.
- Render an inert editor placeholder if layout needs a stable bottom area.
- Do not allow slash autocomplete.
- Do not allow prompt submit.
- Do not route printable input to editor.

This is a structural rule. Do not rely only on `focused={false}`.

## Hints

Hints are derived from role + keymap, not hand-written per component.

Examples:

```ts
selector: "↑/↓ navigate  Enter select  Esc close"
form: "Tab next  Enter submit  Esc cancel"
confirm: "←/→ choose  Enter confirm  Esc cancel"
```

`SurfaceContentRegistry` may provide labels such as "select", "save", or
"delete", but key names must come from `KeymapManager`.

## Migration Plan

1. Add `SurfaceModality` to surface types.
2. Remove `preferredMount` and `targetSlot` from command-authored requests.
3. Add role behavior handlers:
   - `selectorBehavior`
   - `menuBehavior`
   - `formBehavior`
   - `confirmBehavior`
4. Move common list filtering and selection into role behavior state.
5. Make `TuiController` the only interpreter of `SurfaceKeyResult`.
6. Remove direct default Esc handling from surface components.
7. Keep editor-local autocomplete outside `SurfaceManager`.
8. Make blocking/modal surfaces remove real editor input from the render tree.
9. Disable `side-drawer` for command panels until it is a true overlay.
10. Add regression tests:
    - Esc closes top blocking surface.
    - Blocking surface prevents slash autocomplete.
    - `/settings` then `/resume` cannot be opened through editor input.
    - Large blocking selector replaces timeline in render plan.
    - Small/medium blocking panel inserts above editor.
    - Role behavior maps Esc/Enter/Up/Down consistently.

## Acceptance Criteria

The surface subsystem is acceptable when these are true:

- No command sets layout fields.
- No command component contains default `event.name === "escape"` close logic.
- There is one key router for managed surfaces.
- Editor-local autocomplete cannot open while a blocking surface is active.
- `/settings`, `/resume`, `/model`, `/thinking`, `/help`, `/login`, and
  `/notifications` share role behavior instead of custom key handling.
- `npm run check` and host-tui tests cover resolver, render plan, role behavior,
  and command surface requests.

