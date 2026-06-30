#[derive(Default)]
pub struct ScrollViewport {
    pub(super) offset_from_bottom: usize,
    pub(super) pending_new_items: usize,
    pub(super) content_height: usize,
    viewport_height: usize,
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

    pub(super) fn update_metrics(&mut self, content_height: usize, viewport_height: usize) {
        let was_at_latest = self.is_at_latest();
        let old_content_height = self.content_height;
        self.content_height = content_height;
        self.viewport_height = viewport_height.max(1);
        if !was_at_latest && content_height > old_content_height {
            self.offset_from_bottom = self
                .offset_from_bottom
                .saturating_add(content_height - old_content_height);
        }
        self.clamp();
    }

    pub(super) fn max_scroll(&self) -> usize {
        self.content_height.saturating_sub(self.viewport_height)
    }

    pub(super) fn top_offset(&self) -> usize {
        self.max_scroll().saturating_sub(self.offset_from_bottom)
    }

    pub(super) fn pending_new_items(&self) -> usize {
        self.pending_new_items
    }

    pub(super) fn viewport_height(&self) -> usize {
        self.viewport_height
    }

    pub(super) fn scrollbar_position(&self) -> usize {
        let max_scroll = self.max_scroll();
        if max_scroll == 0 {
            return 0;
        }
        self.top_offset()
            .saturating_mul(self.content_height.saturating_sub(1))
            .saturating_add(max_scroll / 2)
            / max_scroll
    }

    fn clamp(&mut self) {
        self.offset_from_bottom = self.offset_from_bottom.min(self.max_scroll());
        if self.offset_from_bottom == 0 {
            self.pending_new_items = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollViewport;

    #[test]
    fn scroll_viewport_clamps_to_content_bounds() {
        let mut viewport = ScrollViewport::default();
        viewport.update_metrics(100, 10);

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
        viewport.update_metrics(100, 10);
        viewport.scroll_up(90);

        viewport.update_metrics(100, 120);
        assert_eq!(viewport.offset_from_bottom, 0);
        assert_eq!(viewport.max_scroll(), 0);
    }

    #[test]
    fn scrollbar_position_tracks_top_and_bottom() {
        let mut viewport = ScrollViewport::default();
        viewport.update_metrics(100, 10);

        viewport.scroll_up(90);
        assert_eq!(viewport.top_offset(), 0);
        assert_eq!(viewport.scrollbar_position(), 0);

        viewport.jump_latest();
        assert_eq!(viewport.top_offset(), 90);
        assert_eq!(viewport.scrollbar_position(), 99);
    }
}
