//! Content-space ownership and live viewport hit resolution.
//!
//! Static [`crate::HitMap`] entries remain responsible for z-order and modal
//! barriers.  This plan is the second stage for scrollable content: it keeps
//! ownership in content coordinates and applies the current viewport offset at
//! event time.

use std::ops::Range;

use ratatui::layout::Rect;

use crate::padding::intersection;

/// One content fragment that owns a half-open column range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentHitFragment<E> {
    pub cols: Range<u16>,
    pub owner: E,
    pub source: Option<Range<usize>>,
}

impl<E> ContentHitFragment<E> {
    pub fn new(cols: Range<u16>, owner: E) -> Self {
        Self {
            cols,
            owner,
            source: None,
        }
    }

    pub fn with_source(mut self, source: Range<usize>) -> Self {
        self.source = Some(source);
        self
    }

    /// A row-wide owner.  The resolver clips the sentinel range to the actual
    /// content width, so callers do not need to know that width when building
    /// semantic row ownership.
    pub fn row_owner(owner: E) -> Self {
        Self::new(0..u16::MAX, owner)
    }
}

/// Ownership fragments for one content-space visual row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentHitRow<E> {
    pub fragments: Vec<ContentHitFragment<E>>,
}

impl<E> Default for ContentHitRow<E> {
    fn default() -> Self {
        Self {
            fragments: Vec::new(),
        }
    }
}

impl<E> ContentHitRow<E> {
    pub fn new(fragments: Vec<ContentHitFragment<E>>) -> Self {
        Self { fragments }
    }

    pub fn owner(owner: E) -> Self {
        Self {
            fragments: vec![ContentHitFragment::row_owner(owner)],
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }
}

/// Content-space ownership for one prepared layout epoch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentHitPlan<E> {
    pub content_rect: Rect,
    pub clip_rect: Rect,
    pub rows: Vec<ContentHitRow<E>>,
    pub epoch: u64,
}

impl<E> ContentHitPlan<E> {
    pub fn new(
        content_rect: Rect,
        clip_rect: Rect,
        rows: Vec<ContentHitRow<E>>,
        epoch: u64,
    ) -> Self {
        Self {
            content_rect,
            clip_rect,
            rows,
            epoch,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Resolve a screen coordinate against a live top-origin viewport offset.
    ///
    /// `cols` in each fragment are relative to `content_rect.x`; the returned
    /// rectangle is screen-space and clipped to the content window.  Empty
    /// rows, padding, clipped cells, and reserved gutters resolve to `None`.
    pub fn resolve(&self, viewport_top: usize, x: u16, y: u16) -> Option<ResolvedContentHit<E>>
    where
        E: Clone,
    {
        let clip = intersection(self.content_rect, self.clip_rect)?;
        if !contains(clip, x, y) {
            return None;
        }
        let row_offset = usize::from(y.saturating_sub(self.content_rect.y));
        let row_index = viewport_top.saturating_add(row_offset);
        let row = self.rows.get(row_index)?;
        let column = x.saturating_sub(self.content_rect.x);
        let content_width = self.content_rect.width;
        let fragment = row.fragments.iter().find(|fragment| {
            let start = fragment.cols.start.min(content_width);
            let end = fragment.cols.end.min(content_width);
            start < end && column >= start && column < end
        })?;

        let start = fragment.cols.start.min(content_width);
        let end = fragment.cols.end.min(content_width);
        let fragment_rect = Rect::new(
            self.content_rect.x.saturating_add(start),
            y,
            end.saturating_sub(start),
            1,
        );
        let rect = intersection(fragment_rect, clip)?;
        let source = fragment.source.as_ref().map(|source| {
            let offset = usize::from(column.saturating_sub(start));
            source.start.saturating_add(offset.min(source.len()))
        });
        Some(ResolvedContentHit {
            owner: fragment.owner.clone(),
            rect,
            source,
        })
    }

    /// Convenience for a range of visible rows, useful when preparing a plan
    /// from a viewport without exposing a separate owner registry.
    pub fn visible_rows(&self, viewport_top: usize, visible_rows: usize) -> Range<usize> {
        let start = viewport_top.min(self.rows.len());
        start..start.saturating_add(visible_rows).min(self.rows.len())
    }
}

/// A semantic owner resolved from content coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedContentHit<E> {
    pub owner: E,
    pub rect: Rect,
    pub source: Option<usize>,
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

/// Build an owner for every row in `rows` without repeating sentinel math at
/// call sites.
pub fn row_owners<E>(rows: impl IntoIterator<Item = Option<E>>) -> Vec<ContentHitRow<E>> {
    rows.into_iter()
        .map(|owner| owner.map_or_else(ContentHitRow::empty, ContentHitRow::owner))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum Owner {
        First,
        Link,
    }

    fn plan() -> ContentHitPlan<Owner> {
        ContentHitPlan::new(
            Rect::new(2, 4, 8, 3),
            Rect::new(2, 5, 8, 2),
            vec![
                ContentHitRow::owner(Owner::First),
                ContentHitRow::new(vec![
                    ContentHitFragment::new(1..4, Owner::Link).with_source(10..13),
                ]),
                ContentHitRow::empty(),
            ],
            7,
        )
    }

    #[test]
    fn resolves_live_offset_and_rejects_padding() {
        let plan = plan();
        // The first content row is clipped by the one-row top overlay.
        assert_eq!(plan.resolve(0, 3, 4), None);
        // Screen row 5 maps to content row 1 at top 0.
        assert_eq!(plan.resolve(0, 3, 5).unwrap().owner, Owner::Link);
        // Moving the viewport changes ownership without changing the plan.
        assert_eq!(plan.resolve(1, 3, 5), None);
        assert_eq!(plan.epoch, 7);
    }

    #[test]
    fn fragment_and_source_are_clipped_to_content() {
        let plan = plan();
        let hit = plan.resolve(0, 3, 5).unwrap();
        assert_eq!(hit.owner, Owner::Link);
        assert_eq!(hit.rect, Rect::new(3, 5, 3, 1));
        assert_eq!(hit.source, Some(10));
        assert_eq!(plan.resolve(0, 2, 5), None);
    }

    #[test]
    fn resolved_fragment_is_clipped_to_the_live_clip() {
        let plan = ContentHitPlan::new(
            Rect::new(0, 0, 8, 1),
            Rect::new(2, 0, 3, 1),
            vec![ContentHitRow::new(vec![ContentHitFragment::new(
                0..8,
                Owner::Link,
            )])],
            1,
        );
        let hit = plan.resolve(0, 2, 0).unwrap();
        assert_eq!(hit.rect, Rect::new(2, 0, 3, 1));
        assert_eq!(plan.resolve(0, 5, 0), None);
    }

    #[test]
    fn ownerless_rows_do_not_fall_through() {
        let plan = plan();
        assert_eq!(plan.resolve(0, 3, 6), None);
        assert_eq!(row_owners([Some(Owner::First), None]).len(), 2);
    }
}
