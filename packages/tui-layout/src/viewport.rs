//! Top-origin viewport state and scrollbar geometry.
//!
//! A viewport knows only rows and terminal rectangles.  Product consumers keep
//! ownership of pending-item counters, cursor policy, selection, and actions.

use std::ops::Range;

use ratatui::layout::Rect;

use crate::padding::{GutterSide, split_gutter};

/// Whether a viewport stays pinned to the last content row.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum ViewportMode {
    /// Keep the current top row when metrics change, clamping when necessary.
    #[default]
    Fixed,
    /// Keep the last visible row at the end of the content.
    FollowEnd,
}

/// Row counts used to clamp a viewport.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ViewportMetrics {
    pub content_rows: usize,
    pub visible_rows: usize,
}

impl ViewportMetrics {
    pub const fn new(content_rows: usize, visible_rows: usize) -> Self {
        Self {
            content_rows,
            visible_rows,
        }
    }

    pub const fn max_scroll(self) -> usize {
        self.content_rows.saturating_sub(self.visible_rows)
    }
}

/// Mutable, top-origin window state.  The plan derived from it is immutable.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ViewportState {
    top: usize,
    mode: ViewportMode,
    metrics: ViewportMetrics,
}

impl ViewportState {
    pub const fn new(content_rows: usize, visible_rows: usize) -> Self {
        Self {
            top: 0,
            mode: ViewportMode::Fixed,
            metrics: ViewportMetrics::new(content_rows, visible_rows),
        }
    }

    pub const fn top(&self) -> usize {
        self.top
    }

    /// Alias used by content hit resolvers.
    pub const fn top_offset(&self) -> usize {
        self.top
    }

    pub const fn mode(&self) -> ViewportMode {
        self.mode
    }

    pub const fn metrics(&self) -> ViewportMetrics {
        self.metrics
    }

    pub const fn content_rows(&self) -> usize {
        self.metrics.content_rows
    }

    pub const fn visible_rows(&self) -> usize {
        self.metrics.visible_rows
    }

    pub const fn max_scroll(&self) -> usize {
        self.metrics.max_scroll()
    }

    /// Update row metrics while preserving the declared anchor.
    pub fn update_metrics(&mut self, metrics: ViewportMetrics) {
        self.metrics = metrics;
        self.top = match self.mode {
            ViewportMode::Fixed => self.top.min(self.max_scroll()),
            ViewportMode::FollowEnd => self.max_scroll(),
        };
    }

    pub fn set_metrics(&mut self, content_rows: usize, visible_rows: usize) {
        self.update_metrics(ViewportMetrics::new(content_rows, visible_rows));
    }

    /// Scroll by signed rows.  Positive values move toward the end.
    ///
    /// Reaching the end elects follow-end, which makes subsequent content
    /// growth stay visible.  Any other movement becomes a fixed viewport.
    pub fn scroll_by(&mut self, delta: isize) {
        if delta == 0 {
            return;
        }
        let next = if delta.is_negative() {
            self.top.saturating_sub(delta.unsigned_abs())
        } else {
            self.top.saturating_add(delta as usize)
        }
        .min(self.max_scroll());
        self.top = next;
        self.mode = if delta > 0 && next == self.max_scroll() {
            ViewportMode::FollowEnd
        } else {
            ViewportMode::Fixed
        };
    }

    pub fn scroll_to(&mut self, top: usize) {
        self.top = top.min(self.max_scroll());
        self.mode = ViewportMode::Fixed;
    }

    /// Scroll to a normalized position in `0.0..=1.0`.
    pub fn scroll_to_fraction(&mut self, fraction: f64) {
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else if fraction.is_sign_positive() {
            1.0
        } else {
            0.0
        };
        let top = (self.max_scroll() as f64 * fraction).round() as usize;
        self.top = top.min(self.max_scroll());
        self.mode = if fraction >= 1.0 {
            ViewportMode::FollowEnd
        } else {
            ViewportMode::Fixed
        };
    }

    pub fn follow_end(&mut self) {
        self.mode = ViewportMode::FollowEnd;
        self.top = self.max_scroll();
    }

    /// Ensure a half-open content-row range is visible.
    pub fn ensure_visible(&mut self, target: Range<usize>) {
        if target.start >= target.end || self.metrics.content_rows == 0 {
            return;
        }
        let start = target.start.min(self.metrics.content_rows);
        let end = target.end.min(self.metrics.content_rows).max(start);
        let visible = self.metrics.visible_rows;
        if visible == 0 {
            self.scroll_to(start);
            return;
        }
        let next = if end.saturating_sub(start) > visible || start < self.top {
            start
        } else if end > self.top.saturating_add(visible) {
            end.saturating_sub(visible)
        } else {
            self.top
        };
        self.scroll_to(next);
    }

    pub fn visible_range(&self) -> Range<usize> {
        let top = self.top.min(self.max_scroll());
        top..top
            .saturating_add(self.metrics.visible_rows)
            .min(self.metrics.content_rows)
    }

    /// Derive a paint/hit plan.  `gutter_width` is reserved even when the
    /// content fits, so wrapping width is stable across the overflow edge.
    pub fn prepare(&self, outer: Rect, gutter_width: u16) -> ViewportPlan {
        prepare_viewport(self, outer, gutter_width)
    }
}

/// Scrollbar geometry for one prepared viewport.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ScrollbarMetrics {
    pub track: Rect,
    pub thumb: Rect,
    pub content_rows: usize,
    pub visible_rows: usize,
    pub top: usize,
    pub max_scroll: usize,
}

impl ScrollbarMetrics {
    /// Convert the top-origin window offset to the item position expected by
    /// Ratatui's scrollbar state. The last viewport therefore maps to the
    /// last content item, while `top` remains the canonical window offset.
    pub fn content_position(&self) -> usize {
        if self.max_scroll == 0 || self.content_rows == 0 {
            return 0;
        }
        let numerator = (self.top.min(self.max_scroll) as u128)
            .saturating_mul(self.content_rows.saturating_sub(1) as u128)
            .saturating_add((self.max_scroll / 2) as u128);
        (numerator / self.max_scroll as u128).min(usize::MAX as u128) as usize
    }

    /// Map a track cell to a top row.  The result is clamped and deterministic
    /// at both ends of the track.
    pub fn top_for_track_row(&self, y: u16) -> usize {
        if self.max_scroll == 0 || self.track.height == 0 {
            return 0;
        }
        let row = usize::from(y.saturating_sub(self.track.y))
            .min(usize::from(self.track.height.saturating_sub(1)));
        let travel = usize::from(self.track.height.saturating_sub(self.thumb.height));
        if travel == 0 {
            return 0;
        }
        (row.min(travel) * self.max_scroll + travel / 2) / travel
    }
}

/// Immutable geometry consumed by paint and hit resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewportPlan {
    pub outer: Rect,
    pub content: Rect,
    pub gutter: Rect,
    pub visible: Range<usize>,
    pub scrollbar: Option<ScrollbarMetrics>,
}

/// Prepare a viewport from top-origin state.
pub fn prepare_viewport(state: &ViewportState, outer: Rect, gutter_width: u16) -> ViewportPlan {
    let (content, gutter) = split_gutter(outer, GutterSide::Right, gutter_width);
    let visible_rows = state.visible_rows().min(usize::from(content.height));
    let max_scroll = state.content_rows().saturating_sub(visible_rows);
    let top = match state.mode() {
        ViewportMode::Fixed => state.top().min(max_scroll),
        ViewportMode::FollowEnd => max_scroll,
    };
    let visible = top..top.saturating_add(visible_rows).min(state.content_rows());
    let scrollbar = (state.content_rows() > visible_rows && gutter.width > 0)
        .then(|| scrollbar_metrics(gutter, state.content_rows(), visible_rows, top));
    ViewportPlan {
        outer,
        content,
        gutter,
        visible,
        scrollbar,
    }
}

fn scrollbar_metrics(
    track: Rect,
    content_rows: usize,
    visible_rows: usize,
    top: usize,
) -> ScrollbarMetrics {
    let max_scroll = content_rows.saturating_sub(visible_rows);
    let track_rows = usize::from(track.height);
    let thumb_rows = if content_rows == 0 || track_rows == 0 {
        0
    } else {
        (track_rows
            .saturating_mul(visible_rows)
            .saturating_add(content_rows - 1)
            / content_rows)
            .clamp(1, track_rows)
    } as u16;
    let travel = track.height.saturating_sub(thumb_rows);
    let thumb_offset = if max_scroll == 0 || travel == 0 {
        0
    } else {
        ((top.min(max_scroll) as u64 * u64::from(travel) + (max_scroll as u64 / 2))
            / max_scroll as u64) as u16
    };
    ScrollbarMetrics {
        track,
        thumb: Rect::new(
            track.x,
            track.y.saturating_add(thumb_offset),
            track.width,
            thumb_rows,
        ),
        content_rows,
        visible_rows,
        top,
        max_scroll,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_and_follow_end_share_top_origin() {
        let mut state = ViewportState::new(100, 10);
        state.follow_end();
        assert_eq!(state.top(), 90);
        state.update_metrics(ViewportMetrics::new(120, 10));
        assert_eq!(state.top(), 110);
        state.scroll_by(-4);
        assert_eq!(state.top(), 106);
        assert_eq!(state.mode(), ViewportMode::Fixed);
        state.update_metrics(ViewportMetrics::new(130, 10));
        assert_eq!(state.top(), 106);
    }

    #[test]
    fn ensure_visible_and_resize_clamp() {
        let mut state = ViewportState::new(30, 5);
        state.ensure_visible(12..13);
        assert_eq!(state.visible_range(), 8..13);
        state.ensure_visible(2..3);
        assert_eq!(state.visible_range(), 2..7);
        state.update_metrics(ViewportMetrics::new(3, 8));
        assert_eq!(state.visible_range(), 0..3);
    }

    #[test]
    fn zero_delta_preserves_follow_end_and_zero_visible_is_empty() {
        let mut state = ViewportState::new(10, 3);
        state.follow_end();
        state.scroll_by(0);
        assert_eq!(state.mode(), ViewportMode::FollowEnd);
        assert_eq!(state.top(), 7);

        let mut empty = ViewportState::new(10, 0);
        empty.follow_end();
        let plan = empty.prepare(Rect::new(0, 0, 10, 4), 1);
        assert_eq!(plan.visible, 10..10);
        assert_eq!(plan.scrollbar.unwrap().visible_rows, 0);
    }

    #[test]
    fn scrollbar_reserves_gutter_and_maps_edges() {
        let mut state = ViewportState::new(100, 10);
        let top = state.prepare(Rect::new(2, 3, 21, 10), 1);
        assert_eq!(top.content, Rect::new(2, 3, 20, 10));
        assert_eq!(top.gutter, Rect::new(22, 3, 1, 10));
        let metrics = top.scrollbar.unwrap();
        assert_eq!(metrics.thumb.height, 1);
        assert_eq!(metrics.content_position(), 0);
        assert_eq!(metrics.top_for_track_row(metrics.track.y), 0);
        state.follow_end();
        let bottom = state.prepare(Rect::new(2, 3, 21, 10), 1);
        let metrics = bottom.scrollbar.unwrap();
        assert_eq!(metrics.thumb.y, metrics.track.bottom().saturating_sub(1));
        assert_eq!(metrics.content_position(), 99);
        assert_eq!(metrics.top_for_track_row(metrics.track.bottom()), 90);
    }

    #[test]
    fn hidden_scrollbar_keeps_content_width() {
        let state = ViewportState::new(2, 10);
        let plan = state.prepare(Rect::new(0, 0, 10, 3), 1);
        assert_eq!(plan.content.width, 9);
        assert_eq!(plan.scrollbar, None);
    }
}
