# Design: Generic Flex Layout (`piko-tui-layout`)

> Status: accepted direction for crate boundary  
> Feature PRD: [shell-flex-layout.md](../features/shell-flex-layout.md)

## Goal

Provide a reusable ratatui layout package:

```text
Shell (split chrome strip)
   → body Rect
Layout
   • Flex plane Node<R>
   • Modal z-stack ModalLayer<R>
   • FocusManager<T>
   → FramePlan<R> / active T
Client
   • defines R, T, declares trees, paints, routes keys
```

## Generics

| Type param | Bound (typical) | Client supplies |
|------------|-----------------|-----------------|
| `R` | `Copy + Eq + Hash` | Region id enum (or unit structs) |
| `T` | `Copy + Eq` | Focus target enum |

**Forbidden in this crate:** hard-coded product variants (Timeline, Settings,
Session, Approval, …).

## Modules

| Module | Exports |
|--------|---------|
| `flex` | `Node`, `Flex`, `FlexItem`, `FlexSize` (`Fixed`/`Grow`/`Min`/`Percent`/`Vw`/`Vh`), `Axis` |
| `engine` | `solve`, `solve_flex`, `FramePlan`, `LayerPlan` |
| `modal` | `ModalLayer`, `ModalPlacement` |
| `focus` | `FocusManager<T>` |
| `hitmap` | `Component`, `SurfacePanel`, `HitMap`, top-layer hit queries |
| `interaction` | `ComponentHit`, `PointerGesture`, `InteractionState` |
| `shell` | `split_shell`, `ShellChrome`, `ShellSplit` |
| `util` | `inset_horizontal` |

## API sketches

```rust
let ShellSplit { body, chrome } = split_shell(frame, ShellChrome::Bottom { height: 1 });

let plane: Node<MyRegion> = flex_column(vec![
    FlexItem::grow(1, leaf(MyRegion::Main)),
    FlexItem::percent(20, leaf(MyRegion::Composer)), // parent-main %
    // FlexItem::vh(30, …) / FlexItem::vw(40, …) relative to solve root
]);

let modals = vec![ModalLayer {
    placement: ModalPlacement::CoverBody,
    host_band_height: 0,
    tree: leaf(MyRegion::Dialog),
}];

let plan = solve(body, &plane, &modals);
let mut focus = FocusManager::new(MyFocus::Idle);
focus.push(MyFocus::Dialog);
```

## Downstream (piko-tui example)

| Lives in `piko-tui` | Lives in `piko-tui-layout` |
|---------------------|----------------------------|
| `Region`, `SurfaceId`, `AppMode` | Flex / modal / generic focus |
| `compose_plane` / `compose_modals` | `solve`, placement host rect math |
| Product Action mapping and outside-click policy | Generic hit depth and interaction vocabulary |
| Theme-specific hover paint | Component-scoped `InteractionState<E>` |

## Test policy

All unit tests inside this crate use **local dummy enums**. Product integration
tests live in the client.

## Non-goals

- Dependencies on `piko-protocol` / `piko-hostd` / `piko-tui`  
- Product UI widgets  
