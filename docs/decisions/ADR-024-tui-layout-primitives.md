# ADR-024: Use typed prepared plans for TUI layout primitives

> Status: accepted
> Date: 2026-08-28

## Context

The TUI had several local implementations of padding, text wrapping, viewport
offsets, scrollbar geometry, Pane zones, and scrollable content hit regions.
Those implementations could calculate different rectangles for paint and
pointer routing. Timeline also needed content ownership that remains valid
when the viewport scrolls after a frame is prepared, while Editor needs
grapheme-safe source mapping for wrapped text.

The existing shell/flex engine and static frame hitmap already provide the
correct product composition and z-order boundaries. The missing piece is a
small, typed foundation for local geometry and prepared content plans without
introducing a universal retained component tree or moving product policy into
the layout crate.

## Decision

- Add product-neutral `Padding`, `Spacer`, `Gutter`, bounded clipping and
  alignment helpers, local `DividerSplit`, top-origin `ViewportState`/
  `ViewportPlan`, and generic `ContentHitPlan` to `piko-tui-layout`.
- Keep text wrapping, source-position mapping, Pane chrome, and paint adapters
  in `piko-tui`, where terminal text policy, Ratatui styles, theme tokens, and
  product chrome are available.
- Use typed prepared plans. Paint, static hit generation, clipping, visible
  rows, and content-space ownership derive from the same plan. Product code
  remains responsible for stable semantic owners, focus, gestures, actions,
  and feature-specific policies.
- Make top-origin viewport state canonical. `FollowEnd` is an explicit mode;
  Timeline retains pending-new behavior and Editor retains cursor-follow policy
  outside the generic state.
- Keep static frame hitmap routing as the modal/z-order gate. Scrollable
  content resolves from a content-space plan plus the current viewport offset,
  so pure scrolling does not require rebuilding text or row ownership.
- Land the first consumers incrementally: Pane and shared geometry, Diagnostics
  as the read-only surface, Editor text/source mapping, and Timeline viewport,
  scrollbar, and live tool-hit ownership. Keep existing compatibility wrappers
  and the typed `PreparedFrame.timeline` slot until a second interactive live
  plan justifies a shared `live_hits` registry.

## Consequences

- Local layout arithmetic has one bounded vocabulary and reserved scrollbar
  gutters no longer change wrapping width at the overflow threshold.
- Timeline hits use stable tool identities and live top offsets, while modal
  hitmap ordering remains authoritative.
- Editor and read-only diagnostics use the same grapheme-aware wrapping and
  viewport contracts as the migrated surfaces.
- The design remains incremental: selectable lists, a generic live-plan
  registry, and additional read-only consumers can migrate later without
  forcing structured Timeline rows and text layouts into one erased type.
- Some narrowly scoped `dead_code` allowances remain on intentionally public
  foundation APIs whose first consumer is not landed yet. They are item-level,
  documented compatibility/future seams rather than module-wide suppression.
