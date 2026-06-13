# host-tui subsystem docs

Design documentation for piko's TUI subsystems.

## Reading order

1. [overview.md](overview.md) — architecture, subsystem boundaries, package structure
2. [surfaces.md](surfaces.md) — mount strategies, occlusion, z-order, surface manager
3. [surface-ux-contract.md](surface-ux-contract.md) — command panel modality, role keyboard contracts, stack policy
4. [focus.md](focus.md) — focus tree, nested menus, bubbling, restore behavior
5. [focus-interaction-enhancement.md](focus-interaction-enhancement.md) — keyboard gateway, interaction stack, input router
6. [keymap.md](keymap.md) — keybinding IDs, defaults, display, config
7. [commands.md](commands.md) — command registry, slash commands, pi parity
8. [notifications.md](notifications.md) — in-memory session notifications
9. [timeline.md](timeline.md) — streaming behavior, auto-scroll, item rendering
10. [selectors.md](selectors.md) — shared selector shell, model selector
11. [autocomplete.md](autocomplete.md) — slash/file/argument autocomplete providers
12. [hints.md](hints.md) — contextual keybinding hints and placement

## Core rule

Commands declare what surface they need. Runtime subsystems resolve mount strategy, occlusion, focus ownership, layout budget, key handling, hints, and notifications. Individual commands or components should not invent their own UI behavior.
