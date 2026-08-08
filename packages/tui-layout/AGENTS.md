# piko-tui-layout

Generic ratatui **flex layout engine**: planar col/row compositions, z-axis modal
layers, LIFO focus stack, and shell chrome split.

- **Does not** depend on piko product types (no SurfaceId, Timeline, hostd).
- Downstream apps pass their own region id `R` and focus target `T` as generics.

## Docs

- Feature PRD: [`docs/features/shell-flex-layout.md`](docs/features/shell-flex-layout.md)
- Design: [`docs/design/flex-layout-engine.md`](docs/design/flex-layout-engine.md)

## Layout consumer sketch

```rust
use piko_tui_layout::*;

#[derive(Clone, Copy, Hash, Eq, PartialEq)]
enum Region { Main, Bar }

let split = split_shell(frame, ShellChrome::Bottom { height: 1 });
let tree = flex_column(vec![
    FlexItem::grow(1, leaf(Region::Main)),
    FlexItem::percent(15, leaf(Region::Bar)), // or ::vh / ::vw
]);
let plan = solve_flex(split.body, &tree); // body is Vw/Vh root
```
