//! Per-tab presentation state: follow, scroll, composer error (F-43 PR 5).

use super::canvas::BlockExpandPref;
use super::*;
use gpui::FollowMode;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AgentViewLocal {
    pub following: bool,
    pub last_scroll_y: f32,
    pub composer_error: Option<String>,
    pub pending_submission: Option<composer::PendingSubmission>,
    pub block_expand: HashMap<String, BlockExpandPref>,
    /// Per-tab attachment chips (F-47); cleared on accepted submit.
    pub attachments: Vec<composer::Attachment>,
}

impl Default for AgentViewLocal {
    fn default() -> Self {
        Self {
            following: true,
            last_scroll_y: 0.0,
            composer_error: None,
            pending_submission: None,
            block_expand: HashMap::new(),
            attachments: Vec::new(),
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

    pub(super) fn view_attachments(&self) -> Vec<composer::Attachment> {
        self.views
            .get(&self.draft_key)
            .map(|view| view.attachments.clone())
            .unwrap_or_default()
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
            self.timeline_list.set_follow_mode(FollowMode::Tail);
        }
    }
}
