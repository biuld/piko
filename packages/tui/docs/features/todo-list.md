# Agent Todo List (TUI)

> Status: draft
> Parent product: [F-27](../../../../docs/features/F-27-agent-todo-list.md)
> Design: [todo-list.md](../design/todo-list.md)

## Overview

The TUI exposes the viewed agent's current host-projected todo list through an
explicit **`/todo` centered overlay**. Todo state never occupies the plane Dock
Stack. Timeline `todo_*` cards remain audit history and may differ from the
newer projected list.

Product/docs/UI use **todo list** and **todo item**. Protocol types remain
`TodoList` / `TodoItem`; tools remain `todo_*`.

## Behavior contract

- `/todo` is always present in the TUI-local command catalog.
- Invoking it opens a dismissible centered overlay scoped to the currently
  viewed `AgentInstance`.
- The overlay shows progress counts and ordered item rows with status glyphs.
- Completed items are visually de-emphasized; the active item remains prominent.
- Long lists scroll inside the overlay with arrow/page bindings and the mouse
  wheel. Opening the overlay resets its viewport to the top.
- Empty, disabled, missing-session, or missing-projection states still open the
  overlay and show an explicit empty message.
- `Esc` and an outside click close the overlay.
- The surface is read-only: it cannot complete, reorder, edit, or invent items.

## Placement

```text
STREAM                         conversation

                                ┌─ todos ─────────┐
                                │ Todos  1/3 done  │  /todo overlay
                                │ ✓ completed       │
                                │ ▸ active          │
                                │ · pending         │
                                └──────────────────┘

(blank reserved dock row)
Guidance
Composer
BottomBar
```

The overlay is a modal z-layer. It is not a `DockBandOffer`, a
`Region::Todos` plane leaf, BottomBar content, or a permanent Stream pin.

## Acceptance

- [ ] A non-empty todo list does not change Dock Stack height.
- [ ] `/todo` opens the viewed agent's current list in a centered overlay.
- [ ] Switching viewed agents changes the list shown on the next open/render.
- [ ] Long lists scroll and empty state remains understandable.
- [ ] Timeline todo cards remain ordinary historical tool projections.
