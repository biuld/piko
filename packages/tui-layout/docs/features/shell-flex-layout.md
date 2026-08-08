# Shell & Flex Layout Engine

> Status: draft (crate contract)
>
> Crate: `piko-tui-layout`  
> Downstream example: any ratatui client (e.g. `piko-tui`)

## Overview

This crate owns a **product-agnostic** terminal layout and focus stack:

1. **Shell chrome split** — reserve a strip (typically bottom), return **body** rect.
2. **Layout engine** — planar **flex** (column/row), optional **z-axis modal** layers,
   solve into rect maps. Region ids and focus targets are **generic**.
3. **Downstream** — declares flex trees + modal stacks, paints widgets into rects,
   maps focus targets to key routing.

The crate **does not know** product meanings: no sessions, no “settings panel”,
no host protocol types. Clients supply their own region enums and focus enums
via type parameters.

## Shell

```text
split_shell(frame, Bottom { height: 1 })
  → body  (layout owns this)
  → chrome (client paints status strip)
```

## Layout

### Plane (flex primitives)

Generic over region id `R`:

| Primitive | Role |
|-----------|------|
| `Node<R>` | `Leaf(R)` or nested `Flex` |
| `Flex { direction, children }` | Column or Row |
| `FlexItem { size, child }` | Fixed / Grow / Min / **Percent** / **Vw** / **Vh** |
| `solve_flex(area, &Node<R>)` | `HashMap<R, Rect>`; `area` is viewport root for Vw/Vh |

### Sizing units (main axis)

| `FlexSize` | Meaning |
|------------|---------|
| `Fixed(n)` | `n` cells |
| `Grow { weight, min }` | fill remaining free space |
| `Min(n)` | at least `n` cells |
| `Percent(p)` | `p%` of **parent flex** main-axis length (0..=100) |
| `Vw(p)` | `p%` of **root** width (CSS-like `vw`) → main-axis length |
| `Vh(p)` | `p%` of **root** height (CSS-like `vh`) → main-axis length |

Helpers: `FlexItem::percent` / `::vw` / `::vh`.

### Z-axis (modals)

`ModalLayer<R>`: placement (`CoverBody` / `ComposerBand` / `Centered`) + flex
tree rooted in that host rect. Solved as layers on top of the plane.

### Focus

`FocusManager<T>`: LIFO stack of client-defined targets `T`. Base value is
passed to `FocusManager::new(base)`.

## Non-goals

- Product surface catalogs  
- SPA routing  
- Full CSS flexbox  
- Free absolute drawing outside solved rects  

## Acceptance

- [ ] Crate builds with **only** `ratatui` dependency  
- [ ] No `piko-*` dependencies  
- [ ] Generic `R` / `T` used for regions and focus  
- [ ] Tests use local dummy enums, not product ids  
