//! Timeline viewport adapter.
//!
//! The generic top-origin state lives in `piko-tui-layout`.  Timeline keeps
//! only its product-specific pending-new counter and the compatibility
//! bottom-origin accessors used by existing rendering code.

use std::cell::Cell;

use piko_tui_layout::{ViewportMetrics, ViewportMode, ViewportPlan, ViewportState};
use ratatui::layout::Rect;

pub struct ScrollViewport {
    /// Compatibility mirror for tests and old callers.  The canonical state
    /// is `state`, and this value is synchronized after mutable operations or
    /// an explicit metrics commit.
    pub(super) offset_from_bottom: usize,
    pub(super) pending_new_items: usize,
    state: Cell<ViewportState>,
}

impl Default for ScrollViewport {
    fn default() -> Self {
        let mut state = ViewportState::default();
        // A new transcript starts at its latest row.
        state.follow_end();
        Self {
            offset_from_bottom: 0,
            pending_new_items: 0,
            state: Cell::new(state),
        }
    }
}

impl ScrollViewport {
    pub(super) fn scroll_up(&mut self, amount: usize) {
        let mut state = self.state.get();
        state.scroll_by(-(amount.min(isize::MAX as usize) as isize));
        self.state.set(state);
        self.sync_offset();
    }

    pub(super) fn scroll_down(&mut self, amount: usize) {
        let mut state = self.state.get();
        state.scroll_by(amount.min(isize::MAX as usize) as isize);
        self.state.set(state);
        self.sync_offset();
        if self.is_at_latest() {
            self.pending_new_items = 0;
        }
    }

    pub(super) fn jump_latest(&mut self) {
        let mut state = self.state.get();
        state.follow_end();
        self.state.set(state);
        self.offset_from_bottom = 0;
        self.pending_new_items = 0;
    }

    pub(super) fn is_at_latest(&self) -> bool {
        self.state.get().mode() == ViewportMode::FollowEnd || self.max_scroll() == 0
    }

    pub(super) fn mark_appended(&mut self) {
        if !self.is_at_latest() {
            self.pending_new_items = self.pending_new_items.saturating_add(1);
        }
    }

    /// Store and immediately apply geometry metrics.  The state itself is
    /// interior-mutable because render receives the Timeline by shared ref;
    /// `apply_metrics` remains as a lifecycle compatibility no-op/commit.
    pub(super) fn set_metrics(&self, content_height: usize, viewport_height: usize) {
        let mut state = self.state.get();
        state.update_metrics(ViewportMetrics::new(content_height, viewport_height.max(1)));
        self.state.set(state);
    }

    pub(crate) fn apply_metrics(&mut self) {
        self.sync_offset();
        if self.is_at_latest() {
            self.pending_new_items = 0;
        }
    }

    pub(super) fn max_scroll(&self) -> usize {
        self.state.get().max_scroll()
    }

    pub(crate) fn top_offset(&self) -> usize {
        self.state.get().top()
    }

    pub(super) fn pending_new_items(&self) -> usize {
        self.pending_new_items
    }

    pub(super) fn prepare(&self, outer: Rect) -> ViewportPlan {
        self.state.get().prepare(outer, 1)
    }

    fn sync_offset(&mut self) {
        self.offset_from_bottom = self.max_scroll().saturating_sub(self.top_offset());
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollViewport;
    use ratatui::layout::Rect;

    #[test]
    fn scroll_viewport_clamps_to_content_bounds() {
        let mut viewport = ScrollViewport::default();
        viewport.set_metrics(100, 10);
        viewport.apply_metrics();

        viewport.scroll_up(1_000);
        assert_eq!(viewport.offset_from_bottom, 90);
        assert_eq!(viewport.top_offset(), 0);

        viewport.scroll_down(1_000);
        assert_eq!(viewport.offset_from_bottom, 0);
        assert_eq!(viewport.top_offset(), 90);
    }

    #[test]
    fn scroll_viewport_clamps_after_resize() {
        let mut viewport = ScrollViewport::default();
        viewport.set_metrics(100, 10);
        viewport.apply_metrics();
        viewport.scroll_up(90);

        viewport.set_metrics(100, 120);
        viewport.apply_metrics();
        assert_eq!(viewport.offset_from_bottom, 0);
        assert_eq!(viewport.max_scroll(), 0);
    }

    #[test]
    fn prepared_scrollbar_tracks_top_and_bottom() {
        let mut viewport = ScrollViewport::default();
        viewport.set_metrics(100, 10);
        viewport.apply_metrics();

        viewport.scroll_up(90);
        assert_eq!(viewport.top_offset(), 0);
        let top = viewport.prepare(Rect::new(0, 0, 20, 10));
        assert_eq!(top.scrollbar.unwrap().top, 0);

        viewport.jump_latest();
        assert_eq!(viewport.top_offset(), 90);
        let bottom = viewport.prepare(Rect::new(0, 0, 20, 10));
        assert_eq!(bottom.scrollbar.unwrap().top, 90);
    }

    #[test]
    fn scrolled_up_growth_keeps_top_offset_before_apply_metrics() {
        let mut viewport = ScrollViewport::default();
        viewport.set_metrics(100, 10);
        viewport.apply_metrics();
        viewport.scroll_up(20);
        let pinned = viewport.top_offset();

        viewport.set_metrics(106, 10);
        assert_eq!(
            viewport.top_offset(),
            pinned,
            "streamed lines must not shift a pinned viewport before Tick"
        );
        viewport.apply_metrics();
        assert_eq!(viewport.top_offset(), pinned);
        assert_eq!(viewport.offset_from_bottom, 26);
    }

    #[test]
    fn pending_banner_viewport_shrink_does_not_shift_content() {
        let mut viewport = ScrollViewport::default();
        viewport.set_metrics(100, 10);
        viewport.apply_metrics();
        viewport.scroll_up(20);
        let pinned = viewport.top_offset();

        viewport.mark_appended();
        viewport.set_metrics(100, 9);
        assert_eq!(viewport.top_offset(), pinned);
        viewport.apply_metrics();
        assert_eq!(viewport.top_offset(), pinned);
    }

    #[test]
    fn bottom_pin_still_follows_new_content() {
        let mut viewport = ScrollViewport::default();
        viewport.set_metrics(100, 10);
        viewport.apply_metrics();
        assert_eq!(viewport.top_offset(), 90);

        viewport.set_metrics(108, 10);
        assert_eq!(viewport.top_offset(), 98);
        viewport.apply_metrics();
        assert_eq!(viewport.offset_from_bottom, 0);
        assert_eq!(viewport.top_offset(), 98);
    }
}
