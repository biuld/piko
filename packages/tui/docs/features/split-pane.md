# Split Pane

> Status: implemented 2026-09-05; see [verification](../../../../docs/verification/F-52-history-ui-refinement.md)
> First consumer: [Session History](../../../../docs/features/F-52-session-history-inspector.md)

## Overview

Split Pane is a reusable content component for two related panes inside one
surface. It provides consistent side-by-side presentation on wide terminals
and a single visible pane on narrow terminals. It has no knowledge of sessions,
lenses, lists, detail fetching, or navigation breadcrumbs.

## Behavior contract

- The caller supplies two content panes, preferred split sizing, minimum usable
  widths, inset policy, active pane, and the pane to show in compact mode.
- Both panes are shown only when their minimum widths, insets, and separator
  fit. Otherwise the caller-designated compact pane fills the content area.
  A pane's minimum width refers to usable content after its insets.
- The component reports wide/compact mode and the visible pane rectangles.
  Layout changes do not mutate caller selection, fetch content, or reset scroll.
- Insets and separator are consistent; compose inside an existing surface
  frame without adding two mandatory nested frames.
- Paint and hit testing consume one prepared plan. Hidden panes and separator
  padding do not receive content input. Zero-sized areas are safe.
- Active-pane styling is distinct from row selection and hover. The component
  receives active-pane state rather than maintaining a competing focus owner.
- Pointer targeting identifies the pane under the pointer; keyboard routing
  uses the caller's active pane. The caller translates gestures into actions.
- Each pane can use its own viewport. Rendering one pane must not update the
  other's viewport or selection. The component does not own content scroll.
- Effective pane-navigation bindings are supplied by the surface. The
  component does not reserve Tab, Escape, or any other product key.

## Non-goals

- Domain selection, list/detail models, paging, requests, and error semantics.
- Global modal focus, a second shell layout engine, or nested surface stacks.
- Draggable separators, persisted ratios, arbitrary docking, or more than two
  panes in the first version.

## Acceptance criteria

- [x] Wide/compact transitions preserve caller state and issue no data requests.
- [x] Minimum widths, nonzero origins, insets, tiny and zero areas produce
      bounded, non-overlapping geometry shared by paint and pointer routing.
- [x] Selection, hover, and active-pane styling remain distinguishable.
- [x] Independent viewport consumers retain their positions across pane focus
      changes and resize; only legal scroll-bound clamping is allowed.
- [x] History adopts the component without private split-geometry duplication.
