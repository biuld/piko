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
3. **Component interaction primitives** — resolved component hits, normalized
   pointer gestures, scoped interaction state, and top-layer hit queries.
4. **Downstream** — declares flex trees + modal stacks, paints widgets into rects,
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

### Pointer readiness (hit contract)

`FramePlan<R>` is the **single geometry authority**: plane rects plus ordered
modal layer rects. A derived hit-test over those rects answers "which region
owns cell (x, y)" without a second region table:

```rust
impl<R: Copy + Eq + Hash> FramePlan<R> {
    /// Region-level z-hit. Ratatui cell semantics:
    /// x in [rect.x, rect.x + rect.width).
    pub fn hit_test(&self, x: u16, y: u16) -> Option<(R, Option<usize>)>;
}

pub struct HitRegion<R, E> {
    pub region: R,
    pub rect: Rect,
    pub element: Option<E>, // None = surface default action
}

pub struct Hit<R, E> {
    pub region: R,
    pub element: Option<E>,
    pub rect: Rect,
    pub z: u16,               // plane = 0, layer i = i + 1
    pub layer: Option<usize>,
}

pub struct HitMap<R, E> { pub hits: Vec<Hit<R, E>> }

pub fn build_hitmap<R, E, F>(
    plan: &FramePlan<R>,
    regions: F, // FnMut(R, Rect) -> Vec<HitRegion<R, E>>
) -> HitMap<R, E>;
```

Rules:

- Layers are scanned in reverse solve order (last painted wins), then the
  plane.
- `HitMap::hit_test(x, y)` returns the top-most entry; within one `z` an
  element beats its surface-default entry.
- `HitMap::top_layer`, `is_top_layer_hit`, and `hit_test_top_layer` expose
  generic modal-depth facts without assigning dismiss/block policy.
- Hit resolution remains a pure function of solved rects; it has no product
  types or business actions.
- Clients may declare per-surface sub-regions (rows, tabs, buttons) relative
  to a surface rect via `SurfacePanel::hit_regions`; the engine does not model
  sub-regions.

The **component contract** also lives here: a generic `Component<E, C>` base
trait (`render` / `render_with_state` + `component_regions` + `focusable`)
shared by every
drawable/hittable piece, plus `SurfacePanel<R, E, C>: Component<E, C>`
stamping region ids on top of it. `R`/`E` are product region/element ids, `C`
is a product render context; `InteractionState<E>` carries component-scoped
hover identity. `ComponentHit<E>` and `PointerGesture` are product-neutral
input vocabulary. The crate references no product types or actions.

No app-level facade type: the crate stays pure functions + data (`solve`,
`FramePlan`, `build_hitmap`, `HitMap`, `hit_test`, `FocusManager<T>`) and the
client's `AppState` is the composition root calling them each frame.
Composition policy (which plane/modal, metrics, push guards) stays in the
client.

Terminal event capture, Region dispatch, and business response remain out of
scope for this crate. Product design:
[`../../../tui/docs/design/modal-hitmap-architecture.md`](../../../tui/docs/design/modal-hitmap-architecture.md).

## Non-goals

- Product surface catalogs  
- SPA routing  
- Full CSS flexbox  
- Free absolute drawing outside solved rects  
- Terminal input adapters and product pointer behavior

## Acceptance

- [ ] Crate builds with **only** `ratatui` dependency  
- [ ] No `piko-*` dependencies  
- [ ] Generic `R` / `T` used for regions and focus  
- [ ] Tests use local dummy enums, not product ids  
- [x] `FramePlan::hit_test` and `HitMap::hit_test` covered by unit tests
      (layer priority, surface default, edges, no-hit)
- [x] Top-layer hit queries and product-neutral interaction primitives are
      covered by local dummy-type tests
