//! Per-tab presentation state: follow, scroll, composer error (F-43 PR 5).

use gpui::point;

use super::*;

#[derive(Debug, Clone)]
pub struct AgentViewLocal {
    pub following: bool,
    pub last_scroll_y: f32,
    pub composer_error: Option<String>,
    pub pending_submission: Option<composer::PendingSubmission>,
}

impl Default for AgentViewLocal {
    fn default() -> Self {
        Self {
            following: true,
            last_scroll_y: 0.0,
            composer_error: None,
            pending_submission: None,
        }
    }
}

impl Shell {
    pub(super) fn view_local(&mut self) -> &mut AgentViewLocal {
        self.views.entry(self.draft_key.clone()).or_default()
    }

    pub(super) fn following(&self) -> bool {
        self.views
            .get(&self.draft_key)
            .map(|view| view.following)
            .unwrap_or(true)
    }

    pub(super) fn composer_error(&self) -> Option<String> {
        self.views
            .get(&self.draft_key)
            .and_then(|view| view.composer_error.clone())
    }

    pub(super) fn pending_submission(&self) -> Option<&composer::PendingSubmission> {
        self.views
            .get(&self.draft_key)
            .and_then(|view| view.pending_submission.as_ref())
    }

    pub(super) fn switch_view_local(&mut self, next_key: &str) {
        if next_key == self.draft_key {
            return;
        }
        let y = f32::from(self.scroll.offset().y);
        let following = self.following();
        {
            let outgoing = self.views.entry(self.draft_key.clone()).or_default();
            outgoing.last_scroll_y = y;
            outgoing.following = following;
        }
        let incoming = self.views.entry(next_key.to_string()).or_default().clone();
        if incoming.following {
            self.scroll.scroll_to_bottom();
        } else {
            let max = f32::from(self.scroll.max_offset().y);
            if max > 0.0 {
                self.scroll
                    .set_offset(point(px(0.), px(incoming.last_scroll_y)));
            }
        }
    }
}
