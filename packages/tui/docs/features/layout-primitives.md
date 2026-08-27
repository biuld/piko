# TUI layout primitives

> Status: implemented (foundation and current consumers; registry/list follow-ups deferred)
>
> Related contracts:
> [shell-flex-layout](../../../tui-layout/docs/features/shell-flex-layout.md),
> [component-feedback](./component-feedback.md),
> [pane-chrome](./pane-chrome.md), [line-layout](./line-layout.md), and
> [pointer-hit-validity](./pointer-hit-validity.md).

## Overview

TUI layout primitives are the shared, product-independent building blocks for
organizing terminal cells inside a solved product region. They define local
geometry, visual separation, padding, text flow, clipping, and scrollable
windows so product components such as Timeline, Editor, lists, and diagnostics
do not privately reimplement those rules.

The primitives sit below feature components and below Pane-specific product
chrome, while complementing the existing shell/flex engine:

```text
shell / flex / modal solve
        |
        v
product region Rect
        |
        v
padding / divider / pane / text flow / viewport
        |
        +--> paint geometry
        `--> pointer geometry
```

Layout and interaction geometry are one contract. A component must not paint
from one calculation and derive hit regions from another.

This contract owns geometry and spatial validity. Semantic colors, focus,
selected/active feedback, and outcome states remain governed by
[component-feedback](./component-feedback.md).

## Terminology

- **Region layout** divides the terminal into product-owned regions and modal
  layers. It is owned by `piko-tui-layout`'s existing shell and flex engine.
- **Local layout** divides or insets one solved region for component content.
- **Visual row** is one terminal row after column-aware wrapping.
- **Content space** addresses the complete laid-out content before viewport
  clipping.
- **Screen space** addresses painted terminal cells.
- **Prepared layout** is the shared result consumed by paint and hit
  resolution.
- **Owner** is the stable semantic identity associated with an interactive
  row or fragment.

## Foundation catalog

### Geometry

The shared geometry vocabulary includes:

| Primitive | Contract |
|-----------|----------|
| Padding / Inset | Remove explicit top/right/bottom/left cell budgets from an area using saturating arithmetic. |
| Spacer | Reserve cells without paint or interaction. |
| Gutter | Reserve a named side band, such as a scrollbar gutter, while keeping content width stable. |
| Divider split | Produce two child areas and an optional divider band without replacing the flex engine. |
| Clip | Bound child paint and hits to the child's assigned area. |
| Align | Place measured content within an area without escaping it. |

Zero-width or zero-height inputs never underflow. When the available area is
smaller than requested chrome, content degrades predictably and all returned
areas remain inside the parent.

### Visual structure

Visual structure includes Divider, Border/Frame, Fill, and Pane.

- A Divider separates two areas but is passive by default.
- A Divider may expose a stable interaction owner only when a feature
  explicitly supports resizing or activation.
- A Pane is the standard composed container for title, optional search,
  content, optional tip, and footer zones.
- Pane owns chrome layout and visual feedback, but not modal placement,
  product focus authority, content scrolling, selection, or business actions.
- Scrollable Pane content is composed as Pane plus Viewport; it is not a
  separate Pane mode.

The detailed Pane chrome profile remains in
[pane-chrome](./pane-chrome.md).

### Text layout

Text layout converts text and styled fragments into visual rows using terminal
display columns.

It must:

- preserve grapheme clusters;
- preserve hard newlines, including empty hard lines;
- wrap by terminal display columns;
- retain styles across wrapped fragments;
- retain source ranges when the input has source text;
- support caller-declared atomic ranges that cannot split across rows;
- map source positions to visual row/column and visual positions back to a
  valid source boundary;
- retain stable owners across every visual fragment produced by wrapping;
- define deterministic behavior for a glyph or atomic range wider than the
  available width.

Truncation, prefix/indent, left/right composition, and trailing reserves use
the same terminal-column policy. The more specific row-composition behavior is
defined by [line-layout](./line-layout.md).

### Viewport

A Viewport maps content-space visual rows into a clipped screen-space window.
It owns generic window state and scrollbar geometry, including:

- top row and maximum scroll;
- fixed-position and follow-end behavior;
- clamped relative and absolute scrolling;
- ensuring a target row range is visible;
- stable content width through a reserved scrollbar gutter;
- visible content range and scrollbar metrics;
- preservation rules when content or viewport dimensions change.

Feature-specific meaning remains outside Viewport:

- Timeline owns pending-new counters and the meaning of “latest”.
- Editor owns cursor, selection, references, and when cursor following resumes.
- Lists own selection and item navigation policy.
- Notifications and Todos own their projected item state.

## Prepared-layout contract

Every composed component has one prepared layout for a given content revision,
available area, and presentation input. That prepared layout is the geometry
authority for:

- measurement exposed to its parent;
- paint areas and clipping;
- visible visual rows;
- child placement;
- static hit regions;
- content-space ownership used by live hit resolution;
- scrollbar track and thumb geometry.

Paint and hit resolution must consume the same prepared layout. Padding,
gutter, divider, clipping, and wrapping must propagate child ownership rather
than requiring a feature to reconstruct hit rectangles.

## Pointer and hit behavior

Two hit-resolution modes coexist.

### Snapshot hits

Static chrome and controls use screen-space regions derived from the prepared
frame. The shared hitmap remains authoritative for:

- region ownership;
- modal depth and outside-modal behavior;
- z-order;
- fixed buttons, title affixes, footer actions, and non-scrolling rows.

### Live content hits

Scrollable content uses content-space ownership plus the current viewport
offset. After the static hitmap establishes that the pointer is inside the
eligible topmost region, the region's content resolver:

1. rejects padding, clipped cells, reserved overlays, and scrollbar gutter;
2. converts the screen row to a content-space visual row;
3. converts the screen column to an owned fragment or valid text position;
4. returns the stable semantic owner, or the component background when no
   owner exists.

Pure scrolling changes only the viewport offset and must not require text or
content layout recomputation. Content, width, wrapping, expansion, or other
row-shape changes invalidate the prepared content layout before subsequent
content resolution.

Owners are stable identities, never visible indices, component slots, or
screen coordinates. Hover, click, and pointer-derived cursor placement use the
same resolver.

The behavioral requirements of
[pointer-hit-validity](./pointer-hit-validity.md) remain normative. This
feature supersedes only its implementation constraint that the generic layout
hit contract remain unchanged: content-space hit vocabulary may now be added
to `piko-tui-layout` when it remains product-independent.

## Composition rules

1. Parent geometry always bounds child paint and hits.
2. Padding, spacer, divider, and empty gutter cells belong to the parent
   background unless explicitly assigned an owner.
3. Children cannot receive pointer hits through another child, a divider,
   clipped content, or modal chrome.
4. A reserved scrollbar gutter remains reserved before and after overflow so
   wrapping does not jump when the scrollbar appears.
5. Pane content is the only area in which a composed content component may
   paint or resolve content hits.
6. Layout primitives never emit product actions. Product components translate
   stable owners and normalized gestures into actions.
7. Focus ownership remains in the product focus stack. A foundation component
   receives focus/interaction state only as presentation input.

## Behavior and interactions

- Wheel and keyboard scrolling use the same viewport state transitions.
- Page movement is based on the current visible-row budget.
- Clicking a scrollbar track may change the viewport only when the consuming
  feature enables that interaction.
- Pointer placement in wrapped editable text resolves to a valid grapheme or
  atomic-range boundary.
- Hover is recomputed from the last pointer position after viewport movement
  and cannot identify content outside the visible clip.
- Resizing preserves the declared anchor: follow-end content remains at the
  end; fixed content preserves its top row when possible; cursor-following
  consumers ensure the cursor is visible after layout.

## Degraded layout

For very small areas:

- all arithmetic saturates;
- content and hit rectangles remain within the parent;
- optional chrome disappears before required content;
- a viewport exposes no hits for rows it cannot paint;
- a too-wide grapheme or atomic item is kept whole and clipped rather than
  split into invalid text;
- zero-size areas paint nothing and expose no child hit regions.

## Configuration

No new user-facing configuration. Theme tokens and feature-specific settings
continue to control paint and product behavior.

## Implementation evidence

The current implementation lands the foundation and its first product
consumers:

- `piko-tui-layout` provides `Padding`, `Spacer`, `Gutter`, `DividerSplit`,
  `ViewportState`/`ViewportPlan`, and `ContentHitPlan`.
- `piko-tui` provides the run-based `text_layout` kernel, prepared Pane
  geometry and paint, and the `ScrollView` paint adapter.
- Timeline uses the shared viewport, scrollbar, and content-space tool-hit
  plan; Editor uses the shared text layout and source-position mapping;
  Diagnostics uses shared text wrapping, viewport state, and ScrollView paint.
- The existing static frame hitmap and Timeline-specific prepared-frame slot
  remain compatibility boundaries. A generic live-hit registry and selectable
  list migration are intentionally follow-up work, not part of this landing.

The layout crate tests and the complete no-default-features TUI test suite
cover the bounded geometry, Unicode wrapping, source mapping, viewport
transitions, scrollbar gutter stability, Pane alignment, and live Timeline
hit behavior.

## Acceptance criteria

- Padding, gutter, divider, and Pane return bounded, non-overlapping areas for
  normal and degenerate terminal sizes.
- Pane paint, content placement, title/footer hits, and child clipping derive
  from one prepared Pane layout.
- Plain text, styled text, CJK, emoji sequences, combining characters, hard
  newlines, and atomic editor references wrap deterministically.
- Text layout round-trips valid source positions through visual coordinates.
- Viewport scrolling, resizing, follow-end, ensure-visible, and scrollbar
  metrics share one tested state model.
- Static controls continue to respect modal z-order through the frame hitmap.
- Scrollable rows resolve against live viewport state and stable content
  owners without full layout recomputation on pure scroll.
- Padding, divider, gap, clipped rows, and scrollbar gutter do not leak hits to
  child content.
- Hover, click, and editor pointer placement use the same prepared geometry as
  paint.
- Timeline, Editor, and at least one read-only/list surface can consume the
  primitives without changing their product behavior.

## Non-goals

- Replacing the shell/flex/modal engine.
- Full CSS box, grid, flexbox, or retained scene-graph semantics.
- Defining Timeline message appearance or Editor editing behavior.
- Moving product ids, actions, modal policy, or focus authority into the
  layout crate.
- Text selection, draggable splitters, or draggable scrollbars in the first
  implementation.
- Forcing text, item-list, and structured-block consumers into one product
  component type.
