# host-tui subsystem docs

Design documentation for piko's TUI subsystems. Updated to reflect the current
implementation as of June 2026.

## Reading order

1. [overview.md](overview.md) — architecture, subsystem boundaries, package structure, TuiController, state model
2. [surfaces.md](surfaces.md) — panel surface lifecycle, placement strategies, render plan, z-order
3. [surface-ux-contract.md](surface-ux-contract.md) — surface interaction contract, key routing, panel routing, editor availability
4. [focus.md](focus.md) — focus tree, FocusManager, InputRouter, interceptors, global handler
5. [focus-interaction-enhancement.md](focus-interaction-enhancement.md) — keyboard gateway, interaction stack, Esc/Enter/printable routing
6. [keymap.md](keymap.md) — keybinding IDs, defaults, display, config
7. [commands.md](commands.md) — command registry, slash commands, pi parity
8. [notifications.md](notifications.md) — in-memory session notifications
9. [timeline.md](timeline.md) — TimelineProjection, deterministic ordering, scroll state, streaming behavior
10. [selectors.md](selectors.md) — shared selector shell, ModelSelector, ThinkingSelector, panel integration
11. [autocomplete.md](autocomplete.md) — slash/file/argument autocomplete providers
12. [hints.md](hints.md) — contextual keybinding hints and placement
13. [agent-panel.md](agent-panel.md) — agent activity view model, row building, column layout, queue rendering

## Core subsystems (source layout)

| Subsystem | Directory | Key files |
|---|---|---|
| `keymap` | `src/keymap/` | `keymap-manager.ts`, `defaults.ts`, `types.ts` |
| `commands` | `src/commands/` | `command-registry.ts`, `builtin-commands/`, `types.ts` |
| `notifications` | `src/notifications/` | `notification-center.ts`, `notification-selectors.ts`, `types.ts` |
| `timeline` | `src/timeline/` | `projection.ts`, `types.ts`, `timeline-builder.ts`, `scroll-controller.ts` |
| `focus` | `src/focus/` | `focus-manager.ts`, `input-router.ts`, `key-normalize.ts`, `types.ts` |
| `surfaces` | `src/surfaces/` | `surface-manager.ts`, `render-plan.ts`, `types.ts` |
| `panels` | `src/panels/` | `panel-runtime.ts`, `panel-factories.ts`, `panel-actions.ts`, `types.ts` |
| `agents` | `src/agents/` | `agent-panel-model.ts`, `agent-panel-layout.ts`, `types.ts` |
| `actions` | `src/actions/` | `session-actions.ts` |
| `autocomplete` | `src/autocomplete/` | `combined-provider.ts`, `slash-provider.ts`, `file-provider.ts`, `types.ts` |
| `editor` | `src/editor/` | `editor-autocomplete-controller.ts`, `editor-autocomplete-state.ts` |
| `layout` | `src/layout/` | `model.ts`, `measure.ts`, `policies.ts`, `bottom-bar-packer.ts` |
| `theme` | `src/theme/` | `resolve.ts`, `schema.ts`, `pi-theme-loader.ts`, `themes/` |
| `state` | `src/state/` | `state.ts`, `events.ts`, `selectors.ts`, `reducers/` |
| `runtime` | `src/runtime/` | `tui-controller.ts` |
| `renderer` | `src/renderer/opentui/` | `App.tsx`, `store.ts`, `action-service.ts`, `Editor.tsx`, `BottomBar.tsx`, `select/`, `timeline/`, `panels/`, `agents/` |

## Core rule

Commands declare what surface they need. Runtime subsystems resolve
placement, input policy, focus ownership, layout budget, key handling, hints,
and notifications. Individual commands or components should not invent their
own UI behavior.
