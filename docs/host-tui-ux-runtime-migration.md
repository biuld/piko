# host-tui UX runtime migration

This document tracks migration from the current OpenTUI/SolidJS implementation to the target subsystem design documented in [packages/host-tui/docs](../packages/host-tui/docs/README.md).

The package docs describe the desired architecture and behavior. This root-level document describes current gaps, file-level migration, and rollout order.

## Current gaps

- `App.tsx` handles keyboard routing, layout application, editor visibility, and overlay rendering.
- `keybinding-registry.ts` mixes keyboard bindings and slash commands.
- `command-dispatcher.ts` opens overlays directly.
- `OverlayContainer` is used for selectors and forms even though those surfaces have different UX contracts.
- `SlashCommandMenu` is visual-only, not a real autocomplete/focus/surface subsystem.
- `ChatView` directly maps transcript items and does not model streaming, scroll anchors, manual scroll intervention, pending new output, or type-specific timeline behavior.
- Status line entries are derived strings with no notification history.
- Selectors rely on native `<select>`, causing focus, height, filtering, and hint issues.
- Current surface placement uses `"modal" | "drawer"` and does not model mount strategy, z-order, or occlusion culling.

## Migration map

| Current file | Target |
|---|---|
| `renderer/opentui/action-service.ts` | `runtime/action-service.ts` |
| `renderer/opentui/keybinding-registry.ts` | split into `keymap/*` and `commands/*` |
| `renderer/opentui/command-dispatcher.ts` | `commands/command-registry.ts` + `TuiController.openCommand()` |
| `renderer/opentui/SlashCommandMenu.tsx` | `renderer/opentui/autocomplete/CommandAutocomplete.tsx` |
| `renderer/opentui/ChatView.tsx` | `renderer/opentui/timeline/TimelineView.tsx` |
| `renderer/opentui/overlays/OverlayContainer.tsx` | replace with mount-specific `SurfaceHost` components |
| `renderer/opentui/overlays/ModelSelector.tsx` | `renderer/opentui/select/ModelSelector.tsx` using `SelectorShell` |
| `renderer/opentui/overlays/ThinkingSelector.tsx` | selector adapter using `SelectorShell` |
| `renderer/opentui/overlays/ResumeSelector.tsx` | selector surface adapter |
| `renderer/opentui/overlays/SettingsSelector.tsx` | nested menu surface adapter |
| `renderer/opentui/App.tsx` | composition shell only |

## Phases

### Phase 0: stop drift

- Keep footer status-only.
- Keep temporary slash menu only as interim UI.
- Do not add more hardcoded key hints.

### Phase 1: keymap parity

- Add `KeymapManager`.
- Add pi-compatible keybinding IDs and defaults.
- Add display and hint helpers.
- Change shortcuts to pi defaults.
- Add conflict detection.

### Phase 2: command registry

- Move slash commands out of `KeybindingRegistry`.
- Add `CommandRegistry`.
- Register all pi builtin slash commands.
- Register piko `/notifications` and `/noti`.
- Mark unimplemented commands as explicit stubs.
- Add prompt template, skill, extension registration hooks.

### Phase 3: notification center

- Add `NotificationCenter`.
- Add `TuiNotificationState` and reducer events.
- Route runtime/command/status warnings through notifications.
- Show latest unexpired notification in `StatusLine`.
- Add notification history surface.

### Phase 4: focus manager

- Add `FocusManager`.
- Route all keyboard input through focus owner.
- Make editor, autocomplete, selectors, forms, and timeline focus owners.
- Add focus restore on close.
- Add nested parent/child focus behavior.

### Phase 5: timeline controller

- Add `TuiTimelineState`.
- Add timeline item model and builder.
- Replace direct transcript rendering with `TimelineView`.
- Add scroll anchor state: bottom, manual, item.
- Add user scroll intervention handling.
- Add pending-new-output indicator.
- Move tool/thinking expansion state into timeline state.

### Phase 6: surface manager

- Replace `TuiOverlayState` with `TuiSurfaceState`.
- Add `SurfaceManager`.
- Add mount strategies: `replace-slot`, `insert-between`, `anchored`, `side-drawer`, `status-line`.
- Derive occlusion from mount, viewport, and content policy.
- Add z-index and occlusion culling so fully covered lower modules do not render.
- Add mount-specific `SurfaceHost` components.
- Route surface open/close through focus manager.
- Add parent-child close semantics.

### Phase 7: selector parity

- Add `SelectorShell`.
- Add `SelectListView`.
- Replace native `<select>` usage.
- Rebuild `ModelSelector` as selector adapter.
- Use keymap-generated local hints.
- Verify narrow/normal/wide terminal layouts.

### Phase 8: autocomplete parity

- Replace temporary `SlashCommandMenu`.
- Add provider API.
- Add selected index, accept/cancel/navigation behavior.
- Add slash argument completion.
- Add file completion.

### Phase 9: command UI parity

- Implement hotkeys surface.
- Implement session info/name.
- Implement scoped models selector.
- Implement session tree/fork/clone/new.
- Implement compact/export/import/share/copy/changelog/reload/logout.

## Smoke checks

- `/` shows anchored autocomplete without hiding editor.
- `/model` opens a surface through `insert-between`, `replace-slot`, or `side-drawer` depending on viewport.
- login opens a blocking form surface without centered modal chrome.
- nested settings menu returns to parent on Esc.
- model selector list/hints/bounds do not overlap.
- streaming output follows bottom only until the user manually scrolls away.
