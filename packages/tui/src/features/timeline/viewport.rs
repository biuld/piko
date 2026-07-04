use std::cell::Cell;

#[derive(Default)]
pub struct ScrollViewport {
    pub(super) offset_from_bottom: usize,
    pub(super) pending_new_items: usize,
    content_height: Cell<usize>,
    viewport_height: Cell<usize>,
    prev_content_height: usize,
}

impl ScrollViewport {
    pub(super) fn scroll_up(&mut self, amount: usize) {
        self.offset_from_bottom = self
            .offset_from_bottom
            .saturating_add(amount)
            .min(self.max_scroll());
    }

    pub(super) fn scroll_down(&mut self, amount: usize) {
        self.offset_from_bottom = self.offset_from_bottom.saturating_sub(amount);
        if self.offset_from_bottom == 0 {
            self.pending_new_items = 0;
        }
    }

    pub(super) fn jump_latest(&mut self) {
        self.offset_from_bottom = 0;
        self.pending_new_items = 0;
    }

    pub(super) fn is_at_latest(&self) -> bool {
        self.offset_from_bottom == 0
    }

    pub(super) fn mark_appended(&mut self) {
        if !self.is_at_latest() {
            self.pending_new_items = self.pending_new_items.saturating_add(1);
        }
    }

    /// Store content height and viewport height from the current render frame.
    /// Called from render (via interior mutability on Cell fields).
    pub(super) fn set_metrics(&self, content_height: usize, viewport_height: usize) {
        self.content_height.set(content_height);
        self.viewport_height.set(viewport_height.max(1));
    }

    /// Apply stored content/viewport metrics and adjust scroll state.
    /// Called from Tick to keep rendering pure.
    pub(crate) fn apply_metrics(&mut self) {
        let content_height = self.content_height.get();
        let viewport_height = self.viewport_height.get();
        let was_at_latest = self.is_at_latest();

        if !was_at_latest && content_height > self.prev_content_height {
            self.offset_from_bottom = self
                .offset_from_bottom
                .saturating_add(content_height - self.prev_content_height);
        }
        self.prev_content_height = content_height;

        // Clamp: ensure offset doesn't exceed max scroll, reset pending at bottom.
        let max_scroll = content_height.saturating_sub(viewport_height);
        self.offset_from_bottom = self.offset_from_bottom.min(max_scroll);
        if self.offset_from_bottom == 0 {
            self.pending_new_items = 0;
        }
    }

    pub(super) fn max_scroll(&self) -> usize {
        self.content_height
            .get()
            .saturating_sub(self.viewport_height.get())
    }

    pub(super) fn top_offset(&self) -> usize {
        self.max_scroll().saturating_sub(self.offset_from_bottom)
    }

    pub(super) fn pending_new_items(&self) -> usize {
        self.pending_new_items
    }

    pub(super) fn viewport_height(&self) -> usize {
        self.viewport_height.get()
    }

    pub(super) fn content_height(&self) -> usize {
        self.content_height.get()
    }

    pub(super) fn scrollbar_position(&self) -> usize {
        let max_scroll = self.max_scroll();
        if max_scroll == 0 {
            return 0;
        }
        self.top_offset()
            .saturating_mul(self.content_height.get().saturating_sub(1))
            .saturating_add(max_scroll / 2)
            / max_scroll
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollViewport;

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
    fn scrollbar_position_tracks_top_and_bottom() {
        let mut viewport = ScrollViewport::default();
        viewport.set_metrics(100, 10);
        viewport.apply_metrics();

        viewport.scroll_up(90);
        assert_eq!(viewport.top_offset(), 0);
        assert_eq!(viewport.scrollbar_position(), 0);

        viewport.jump_latest();
        assert_eq!(viewport.top_offset(), 90);
        assert_eq!(viewport.scrollbar_position(), 99);
    }
}
