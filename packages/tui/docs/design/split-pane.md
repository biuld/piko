# Split Pane Design

> Status: implemented 2026-09-05
> Implements: [Split Pane](../features/split-pane.md)
> Consumer: [D-69](../../../../docs/design/D-69-session-history-inspector.md)

## Ownership

| Layer | Responsibility |
|---|---|
| `piko-tui-layout` | Existing `DividerSplit`/`DividerPlan`, padding and viewport geometry; no product IDs or theme |
| `piko-tui::ui::components::split_pane` | Reusable two-pane composition, compact fallback, prepared regions, separator/inset painting, active-pane feedback |
| Product surface | Pane content, active/compact pane state, input actions, navigation, independent viewports and asynchronous requests |

`Pane` remains the outer surface chrome. Split Pane consumes `PanePlan.content`;
it does not add another global surface or change the shell plane. Reuse the
existing divider painter and semantic theme tokens where available.

## Component contract

The product-independent `PaneSide` enum identifies `First` and `Second`.
A `SplitPaneSpec` carries preferred first-pane size, both minimum content
widths, separator width and insets. A pure preparation function receives the
content rectangle, spec, and caller-designated compact pane, returning a
`SplitPanePlan` with mode, visible outer/content rectangles, optional separator,
and pane hit regions. The implementation is `ui::components::split_pane`; `PaneRegion` carries
outer and content rectangles.

Compute usable widths after insets and separator. If both minimums fit, clamp
the preferred first-pane size between its minimum and the remaining room for
the second; solve the pair with the existing `DividerSplit`. Otherwise use
compact mode. Avoid a separate hard-coded terminal breakpoint in each feature.
All rectangles are bounded by the supplied content area.

Paint, pane-under-pointer resolution, and child clipping consume the same
prepared plan associated with the painted frame. A resize requires a new frame;
events must not combine old painted content with newly guessed geometry.

The caller provides active-pane state from its surface-local focus routing.
Global `FocusManager` remains the authority for the active surface; internal
pane changes do not push another modal. Split Pane exposes pane identities and
styles without installing a competing focus stack. It has no default bindings.

Each child renderer receives its content rectangle and retains its own
`ViewportState`. The component never ensures a child row is visible or writes
content metrics. Caller reducers handle pane activation, back navigation,
selection restoration, and requests. This keeps plain two-pane comparisons
usable without a mandatory list/detail controller.

## History integration

History uses `features/history/layout.rs` to prepare the outer Pane, lens tabs,
Split Pane, list viewport, and row hit regions from one geometry recipe. History maps
first/second to selector/detail and chooses the compact pane from its existing
navigation state. It owns separate selector/detail viewports and opened item
identity. Enter/open may request detail and focus it; Back restores selector
context. Tab/Shift+Tab remain History lens navigation.

## Verification

Test minimum-width clamping, boundary transitions, insets, nonzero origins,
zero areas, and identical paint/hit geometry at the component layer. History
integration tests cover independent scrolling, resize, open/back, late detail
responses, and unchanged lens bindings. Visually verify separator spacing and
active-pane feedback in both layout modes before marking the PRD implemented.
