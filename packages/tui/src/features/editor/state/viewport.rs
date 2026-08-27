//! Editor viewport adapter over the shared top-origin viewport state.

use piko_tui_layout::{ViewportMetrics, ViewportMode, ViewportPlan, ViewportState};
use ratatui::layout::Rect;

/// Product policy layered over generic wrapped-row window state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EditorViewport {
    state: ViewportState,
    follow_cursor: bool,
}

impl Default for EditorViewport {
    fn default() -> Self {
        let mut state = ViewportState::default();
        state.follow_end();
        Self {
            state,
            follow_cursor: true,
        }
    }
}

impl EditorViewport {
    pub(super) fn reset(&mut self) {
        self.state.follow_end();
        self.follow_cursor = true;
    }

    pub(super) fn resume_cursor_follow(&mut self) {
        self.follow_cursor = true;
    }

    pub(super) fn follows_cursor(&self) -> bool {
        self.follow_cursor
    }

    pub(super) fn top_offset(&self, content_height: usize, viewport_height: u16) -> usize {
        let mut state = self.state;
        state.update_metrics(ViewportMetrics::new(
            content_height,
            usize::from(viewport_height.max(1)),
        ));
        state.top()
    }

    /// Prepare shared geometry using the product's effective top row. Cursor
    /// following can choose a temporary top row for paint without mutating
    /// the persisted viewport state.
    pub(super) fn prepare(
        &self,
        outer: Rect,
        content_height: usize,
        viewport_height: u16,
        top_offset: usize,
    ) -> ViewportPlan {
        let mut state = self.with_metrics(content_height, viewport_height);
        state.scroll_to(top_offset);
        state.prepare(outer, 1)
    }

    pub(super) fn scroll_up(&mut self, amount: usize, content_height: usize, viewport_height: u16) {
        let mut state = self.with_metrics(content_height, viewport_height);
        if state.max_scroll() == 0 {
            return;
        }
        self.follow_cursor = false;
        state.scroll_by(-(amount.min(isize::MAX as usize) as isize));
        self.state = state;
    }

    pub(super) fn scroll_down(
        &mut self,
        amount: usize,
        content_height: usize,
        viewport_height: u16,
    ) {
        let mut state = self.with_metrics(content_height, viewport_height);
        if state.max_scroll() == 0 {
            return;
        }
        state.scroll_by(amount.min(isize::MAX as usize) as isize);
        self.follow_cursor = state.mode() == ViewportMode::FollowEnd;
        self.state = state;
    }

    pub(super) fn set_top_offset(
        &mut self,
        top_offset: usize,
        content_height: usize,
        viewport_height: u16,
    ) {
        let mut state = self.with_metrics(content_height, viewport_height);
        state.scroll_to(top_offset);
        self.follow_cursor = false;
        self.state = state;
    }

    fn with_metrics(&self, content_height: usize, viewport_height: u16) -> ViewportState {
        let mut state = self.state;
        state.update_metrics(ViewportMetrics::new(
            content_height,
            usize::from(viewport_height.max(1)),
        ));
        state
    }
}

#[cfg(test)]
mod tests {
    use super::EditorViewport;
    use ratatui::layout::Rect;

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
    fn prepared_scrollbar_maps_viewport_edges_to_content_edges() {
        let mut viewport = EditorViewport::default();
        let outer = Rect::new(0, 0, 10, 4);
        let bottom = viewport.prepare(outer, 10, 4, viewport.top_offset(10, 4));
        assert_eq!(
            bottom.scrollbar.unwrap().top,
            6,
            "follow-end top is the window offset"
        );

        viewport.scroll_up(6, 10, 4);
        let top = viewport.prepare(outer, 10, 4, viewport.top_offset(10, 4));
        assert_eq!(
            top.scrollbar.unwrap().top,
            0,
            "scrolling to the beginning maps to top"
        );
    }
}
