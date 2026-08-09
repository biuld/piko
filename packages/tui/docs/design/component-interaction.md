# Design: Component Pointer Interaction

> Status: accepted
>
> PRD: [../features/component-interaction.md](../features/component-interaction.md)

## Goal

Complete the interactive component contract without coupling product behavior
to `piko-tui-layout` or centralizing feature semantics in the pointer router.

## Generic layout contract

`piko-tui-layout` defines the transport- and product-neutral input vocabulary:

```rust
pub struct ComponentHit<E> {
    pub element: Option<E>,
    pub rect: Rect,
    pub x: u16,
    pub y: u16,
}

pub enum PointerGesture {
    Activate,
    ScrollUp,
    ScrollDown,
}

pub struct InteractionState<E> {
    pub hovered: Option<E>,
}

impl HitMap<R, E> {
    pub fn top_layer(&self) -> Option<usize>;
    pub fn is_top_layer_hit(&self, hit: Option<&Hit<R, E>>) -> bool;
    pub fn hit_test_top_layer(&self, x: u16, y: u16) -> Option<&Hit<R, E>>;
}
```

The generic `Component` paint contract accepts `InteractionState<E>` through
`render_with_state`; its default implementation delegates to the existing
stateless `render` method. Interactive components override it when hover or a
future pointer state affects paint.

## Product behavior contract

`piko-tui/src/ui/interaction.rs` retains only the Action-producing behavior:

```rust
pub trait PointerComponent<E> {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<E>,
        gesture: PointerGesture,
    ) -> Vec<Action>;
}
```

This trait is product-layer because its output is the TUI `Action` vocabulary.
`ComponentHit::local_x` derives the horizontal coordinate relative to the
resolved hit rect for editor-like controls; absolute `x`/`y` remain available
for components that need both axes.

## Routing

```text
Crossterm mouse
  → normalize PointerGesture
  → build hit map + resolve top hit
  → compare hit.layer with current top modal layer
      mismatch → SurfaceId::outside_click_policy
      match    → Region owner PointerComponent::pointer_event
  → Vec<Action>
```

`Region` matching is composition wiring only. Element interpretation occurs in
the component implementation.

## Hover paint

The application retains the resolved `(Region, Option<HitId>)`. Composition
filters it to the region being painted and passes `InteractionState<HitId>` to
the component. Workflow, suggestion, editor, and notice renderers apply hover
tokens inside their own paint path. Selected or active state therefore wins by
the component's normal style precedence rather than a centralized exclusion
table.

## Selectable row geometry

The shared selectable-list component derives visible row rectangles from the
same `PaneSpec`, filter, row layouts, selected index, and viewport rules used
for paint. It returns source-row indices rather than filtered positions. This
keeps hit identity stable while filters and group headers change. Group headers
and empty/loading messages are not actionable.

Feature components map `Row(source_index)` according to their business role:
pickers and menus select then confirm, MCP selects without an effect, Processes
enters the existing two-stage confirmation path, and Tree uses its existing
navigation/fork confirmation path. Wheel changes selection or viewport but
never confirms.

Timeline follows the same geometry-ownership rule with a block render plan.
The plan retains component line ranges before flattening, applies scroll and
viewport clipping once, and exposes hits only for actionable Tool blocks.
Read-only messages and inter-component gaps remain Stream background.

## Modal outside clicks

`SurfaceId::outside_click_policy()` returns `Dismiss` or `Block`. The router
compares the resolved hit's layer with the top layer. A lower-layer or absent
hit is an outside click; it never reaches the lower component.

CoverBody surfaces have no body-area outside rect, but the same rule remains
valid. Surface-default hits inside the host are on the top layer and are
delegated normally.

## Validation

- Component unit tests cover action mapping and hover precedence.
- Pointer integration tests cover outside dismiss, outside block, background
  no-op, click actions, wheel, editor coordinates, and hover tracking.
- Existing keyboard action-dispatch tests remain unchanged.
