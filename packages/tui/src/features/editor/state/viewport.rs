/// Scroll state for the editor's wrapped-line viewport.
///
/// Like the timeline viewport, the state is bottom-origin: zero means the
/// viewport is pinned to the last visible line. Rendering converts it to the
/// top-origin offset required by the editor paragraph and scrollbar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EditorViewport {
    offset_from_bottom: usize,
    follow_cursor: bool,
}

impl Default for EditorViewport {
    fn default() -> Self {
        Self {
            offset_from_bottom: 0,
            follow_cursor: true,
        }
    }
}

impl EditorViewport {
    pub(super) fn reset(&mut self) {
        self.offset_from_bottom = 0;
        self.follow_cursor = true;
    }

    pub(super) fn resume_cursor_follow(&mut self) {
        self.follow_cursor = true;
    }

    pub(super) fn follows_cursor(&self) -> bool {
        self.follow_cursor
    }

    pub(super) fn top_offset(&self, content_height: usize, viewport_height: u16) -> usize {
        max_scroll(content_height, viewport_height).saturating_sub(self.offset_from_bottom)
    }

    /// Convert a viewport top offset to the position expected by ratatui's
    /// scrollbar. A scrollbar position is content-relative rather than a
    /// scroll-window offset, so the latest viewport maps to the last content
    /// item.
    pub(super) fn scrollbar_position_for_top(
        top_offset: usize,
        content_height: usize,
        viewport_height: u16,
    ) -> usize {
        let max_scroll = max_scroll(content_height, viewport_height);
        if max_scroll == 0 {
            return 0;
        }
        top_offset
            .min(max_scroll)
            .saturating_mul(content_height.saturating_sub(1))
            .saturating_add(max_scroll / 2)
            / max_scroll
    }

    pub(super) fn scroll_up(&mut self, amount: usize, content_height: usize, viewport_height: u16) {
        let max = max_scroll(content_height, viewport_height);
        if max == 0 {
            return;
        }
        self.follow_cursor = false;
        self.offset_from_bottom = self.offset_from_bottom.saturating_add(amount).min(max);
    }

    pub(super) fn scroll_down(
        &mut self,
        amount: usize,
        content_height: usize,
        viewport_height: u16,
    ) {
        let max = max_scroll(content_height, viewport_height);
        if max == 0 {
            return;
        }
        self.follow_cursor = false;
        self.offset_from_bottom = self.offset_from_bottom.min(max).saturating_sub(amount);
        if self.offset_from_bottom == 0 {
            self.follow_cursor = true;
        }
    }

    pub(super) fn set_top_offset(
        &mut self,
        top_offset: usize,
        content_height: usize,
        viewport_height: u16,
    ) {
        let max = max_scroll(content_height, viewport_height);
        self.offset_from_bottom = max.saturating_sub(top_offset.min(max));
        self.follow_cursor = false;
    }
}

fn max_scroll(content_height: usize, viewport_height: u16) -> usize {
    content_height.saturating_sub(usize::from(viewport_height.max(1)))
}

#[cfg(test)]
mod tests {
    use super::EditorViewport;

    #[test]
    fn viewport_scrolls_from_bottom_and_clamps() {
        let mut viewport = EditorViewport::default();
        viewport.scroll_up(3, 10, 4);
        assert_eq!(viewport.top_offset(10, 4), 3);

        viewport.scroll_down(2, 10, 4);
        assert_eq!(viewport.top_offset(10, 4), 5);

        viewport.scroll_up(100, 10, 4);
        assert_eq!(viewport.top_offset(10, 4), 0);
    }

    #[test]
    fn setting_top_offset_round_trips() {
        let mut viewport = EditorViewport::default();
        viewport.set_top_offset(2, 10, 4);
        assert_eq!(viewport.top_offset(10, 4), 2);
    }

    #[test]
    fn scrollbar_position_maps_viewport_edges_to_content_edges() {
        let mut viewport = EditorViewport::default();
        assert_eq!(
            EditorViewport::scrollbar_position_for_top(viewport.top_offset(10, 4), 10, 4),
            9
        );

        viewport.scroll_up(6, 10, 4);
        assert_eq!(
            EditorViewport::scrollbar_position_for_top(viewport.top_offset(10, 4), 10, 4),
            0
        );
    }
}
