# Single-line Dock

> Status: reviewed

## Overview

The Single-line Dock is shared chrome for compact, one-row feedback adjacent
to an interaction surface. Notice Row messages and Pane footer key hints use
the same renderer, alignment, clipping, and optional hover backdrop while
retaining their separate state, priority, and input behavior.

This is a **paint helper** (`ui::components::dock_line`), **not** the plane
**Dock Stack** feature that coordinates multiple optional bands (Notice /
Todos / Suggest heights). Stack coexistence:
[dock-coexistence.md](./dock-coexistence.md).

## Layout

```text
● warning · approval requested · F8 dismiss
⌨ ↑/↓ navigate · Enter confirm · Esc cancel
```

The glyph identifies the row's purpose. Attention rows use the severity
color; hint rows use the dim feedback token. A Dock always occupies exactly
one solved row and clips overflow rather than wrapping or changing layout.

## Behavior / interactions

- A Notice Row remains a plane region above the Composer. Its visible record,
  dismissal behavior, and hover hit target remain owned by NotificationCenter.
- A Pane footer remains pane-local passive key guidance. It has no pointer or
  keyboard action of its own.
- Dock rendering does not decide which row is visible or arbitrates priority.
  Notice policy and each surface's focus state continue to do that.
- Empty hint text reserves no footer row. For a non-empty hint value, only its
  first non-empty line is displayed.

## Configuration

No settings or bindings are added. `app.notifications.clear` remains bound to
F8 for a visible Notice Row.

## Non-goals

- Combining notification and hint state into one queue.
- Moving panel hints into shell chrome or the BottomBar.
- Replacing the Composer input row with notices or hints.
