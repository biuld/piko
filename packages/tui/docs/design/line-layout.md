# Design: Line layout primitives

## Selected feature

Implements [line-layout.md](../features/line-layout.md).

## Goal

One reusable place for **terminal-column** row composition so timeline and
other stream chrome share the same left/right rules.

## Constraints and non-goals

- Must not depend on product timeline types.
- Must not live in `piko-tui-layout` (different domain: `Rect` flex vs string
  columns).
- Width = `unicode-width`; no CJK Ambiguous force-wide heuristic.

## Proposed design

### Module

`packages/tui/src/ui/line_layout.rs`

### Column math

| API | Role |
|-----|------|
| `paint_cols` | `UnicodeWidthStr::width` |
| `take_prefix_cols` / `soft_wrap` | wrap by columns |
| `truncate_paint_cols` | hard clip |
| `truncate_cols` | clip + ASCII `...` |
| `trailing_reserve` | affix + mid spacer + edge inset |

### Paint

| API | Role |
|-----|------|
| `filled_line` | single style, pad to width |
| `pad_spans` | pad existing spans |
| `left_right_line` | left + spacer + right + edge |
| `left_column_line` | left + blank reserved right |
| `body_with_trailing` | soft-wrap multi-line body with first-row affix |

### Consumers

Any stream/chrome paint that needs left/right column math. Specific product
layouts (timestamps, tool titles, …) are **not** designed in this file—see
[ui-ux stream principles](../features/ui-ux.md) and presenters.

### Width policy

Reserve and paint both use `paint_cols`. Terminals that paint wider than
`unicode-width` may still clip until a capability-aware policy is productized.

## Package impact

| Package | Change |
|---------|--------|
| `piko-tui` | `ui/line_layout.rs` + timeline/editor consumers |
| `piko-tui-layout` | none |

## Failure and cancellation

N/A (pure layout helpers).
