# ADR-030: Shared TUI split-pane composition

> Status: accepted
> Date: 2026-09-05
> Related: [ADR-024](ADR-024-tui-layout-primitives.md),
> [D-69](../design/D-69-session-history-inspector.md)

## Context

Session History duplicated its selector/detail split in paint and pointer
geometry and shared one viewport between both contents. The same layout and
focus presentation is useful to other two-pane TUI surfaces. The layout crate
already has a product-independent `DividerSplit` primitive; `Pane` already
owns the outer surface frame.

## Decision

Add `ui::components::split_pane` to compose two generic content regions inside
an existing Pane. Reuse `piko-tui-layout::DividerSplit` for geometry. Minimum
usable widths and insets determine compact fallback; a caller supplies which
pane compact mode displays. The prepared plan supplies pane hit regions,
separator painting, and active-pane feedback.

Keep lens navigation, selected/opened identities, requests, independent content
viewports, and local active-pane state in the consuming feature. Global focus
continues to identify one History surface; switching internal panes does not
push another modal. The component reserves no key bindings and has no list or
detail-fetching model.

History prepares one geometry recipe for painting and frame hit-map generation.
Large detail loads only on explicit open. Loaded summaries can be inspected
independently in either layout mode. The inspector's durable authority and
explicit-refresh contract are unchanged.

## Consequences

- Future two-pane surfaces reuse geometry and visual composition without
  adopting History's product state or navigation model.
- Scroll and async ownership remain explicit; painting detail cannot reset
  list selection or scrolling.
- Draggable separators, saved ratios, arbitrary docking, and a new recursive
  layout engine are outside this change.
