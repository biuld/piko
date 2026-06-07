# host-tui UX runtime redesign

This directory contains the OpenTUI/SolidJS TUI redesign plan split by subsystem.

The goal is to define piko's TUI as a small UX runtime. The major subsystems are surface placement, focus routing, timeline rendering/scrolling, layout budgeting, keymaps, commands, notifications, selectors, autocomplete, hints, theme, and renderer boundaries.

## Reading order

1. [overview.md](overview.md) — target architecture, subsystem boundaries, package structure.
2. [surfaces.md](surfaces.md) — mount strategies, derived occlusion, z-order, and surface manager.
3. [surface-ux-contract.md](surface-ux-contract.md) — command panel modality, role keyboard contracts, stack policy, and editor blocking rules.
4. [focus.md](focus.md) — focus tree, nested menus, bubbling, and restore behavior.
5. [focus-interaction-enhancement.md](focus-interaction-enhancement.md) — keyboard gateway, interaction stack, input router, and semantic key results.
6. [keymap.md](keymap.md) — pi-compatible keybinding IDs, defaults, display, config.
7. [commands.md](commands.md) — command registry, slash commands, pi parity, piko-specific commands.
8. [notifications.md](notifications.md) — in-memory session notifications and `/notifications`.
9. [timeline.md](timeline.md) — streaming behavior, auto-scroll, manual scroll, timeline item rendering.
10. [selectors.md](selectors.md) — shared selector shell, SelectListView, model selector redesign.
11. [autocomplete.md](autocomplete.md) — slash/file/argument autocomplete providers and UI.
12. [hints.md](hints.md) — contextual keybinding hints and placement policy.

Migration and current-code rollout live in the root docs:

- [../../../docs/host-tui-ux-runtime-migration.md](../../../docs/host-tui-ux-runtime-migration.md)

## Core rule

Do not let individual commands or components invent their own UI behavior. Commands request roles and content requirements; runtime subsystems resolve surface mount strategy, derived occlusion, focus ownership, layout budget, key handling, hints, and notifications.
