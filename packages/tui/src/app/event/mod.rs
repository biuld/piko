use piko_protocol::{Command, ServerMessage as Event, SessionSnapshot, SessionTreeEntry};

use crate::{
    app::{
        AppMode, AppState, QueueStatus, SurfaceId, command_id, effect::Effect, flatten_models,
        get_active_branch_entries,
    },
    config::TuiConfig,
    features::approval::PendingApproval,
    features::notifications::{NoticePolicy, NoticeScope, NoticeSubject, NotificationLevel},
};

mod events;
mod lifecycle;
mod responses;
mod snapshot;

impl AppState {
    fn with_agent_timeline(
        &mut self,
        agent_instance_id: &str,
        apply: impl FnOnce(&mut crate::features::timeline::Timeline),
    ) {
        let is_active = self
            .agent_panel
            .active_agent_instance_id
            .as_deref()
            .is_none_or(|active| active == agent_instance_id);
        if is_active {
            if self.agent_panel.active_agent_instance_id.is_none() {
                self.agent_panel.active_agent_instance_id = Some(agent_instance_id.to_string());
                self.tree.set_agent_filter(Some(agent_instance_id));
            }
            apply(&mut self.timeline);
        } else {
            if !self.agent_timelines.contains_key(agent_instance_id) {
                let mut timeline = crate::features::timeline::Timeline::new();
                for (entry, order) in &self.session_timeline_entries {
                    let _ = timeline.apply_session_entry(entry.clone(), *order);
                }
                self.agent_timelines
                    .insert(agent_instance_id.to_string(), timeline);
            }
            apply(
                self.agent_timelines
                    .entry(agent_instance_id.to_string())
                    .or_insert_with(crate::features::timeline::Timeline::new),
            );
        }
    }

    fn accepts_session(&self, session_id: &str) -> bool {
        !self.session.initializing && self.session.id.as_deref() == Some(session_id)
    }

    fn accepts_reconcile(&self, session_id: &str) -> bool {
        self.session.opening_id.as_deref().map_or_else(
            || self.session.id.as_deref() == Some(session_id),
            |target| target == session_id,
        )
    }

    fn select_agent_timeline(&mut self, agent_instance_id: &str) {
        if self.agent_panel.active_agent_instance_id.as_deref() == Some(agent_instance_id) {
            self.tree.set_agent_filter(Some(agent_instance_id));
            return;
        }
        if let Some(previous) = self
            .agent_panel
            .active_agent_instance_id
            .replace(agent_instance_id.to_string())
        {
            let mut next_timeline = self.agent_timelines.remove(agent_instance_id);
            if next_timeline.is_none() {
                let mut timeline = crate::features::timeline::Timeline::new();
                for (entry, order) in &self.session_timeline_entries {
                    let _ = timeline.apply_session_entry(entry.clone(), *order);
                }
                next_timeline = Some(timeline);
            }
            let previous_timeline = std::mem::replace(
                &mut self.timeline,
                next_timeline.expect("timeline constructed above"),
            );
            self.agent_timelines.insert(previous, previous_timeline);
        } else {
            self.timeline = if let Some(timeline) = self.agent_timelines.remove(agent_instance_id) {
                timeline
            } else {
                let mut timeline = crate::features::timeline::Timeline::new();
                for (entry, order) in &self.session_timeline_entries {
                    let _ = timeline.apply_session_entry(entry.clone(), *order);
                }
                timeline
            };
        }
        self.tree.set_agent_filter(Some(agent_instance_id));
    }

    pub fn apply_event(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::TranscriptCommitted(committed) => self.apply_transcript_committed(committed),
            Event::SessionEntryCommitted(committed) => {
                self.apply_session_entry_committed(committed)
            }
            Event::StreamItem(patch) => self.apply_stream_item(patch),
            Event::SessionReconciled(reconciled) => self.apply_session_reconciled(reconciled),
            Event::SessionCleared(cleared) => self.apply_session_cleared(cleared),
            Event::AgentChanged(agent) => self.apply_agent_changed(agent),
            Event::Interaction(event) => self.apply_interaction(event),
            Event::TurnDiff(diff) => self.apply_turn_diff(diff),
            Event::CommandResponse { command_id, result } => {
                self.apply_command_response(command_id, result)
            }
            Event::TurnLifecycle(event) => self.apply_turn_lifecycle(event),
            Event::AgentRunLifecycle(_) => Vec::new(),
            Event::Approval(event) => self.apply_approval(event),
            Event::Queue(event) => self.apply_queue(event),
            Event::Auth(event) => self.apply_auth(event),
            Event::Model(event) => self.apply_model(event),
            Event::Usage(event) => self.apply_usage(event),
        }
    }
}

/// Walk the active branch newest-first for the latest assistant prompt-side tokens.
fn last_context_tokens_from_entries(
    entries: &[SessionTreeEntry],
    current_leaf_id: Option<&str>,
) -> Option<u64> {
    let branch = get_active_branch_entries(entries, current_leaf_id);
    for entry in branch.into_iter().rev() {
        if let SessionTreeEntry::Message(message_entry) = entry
            && let piko_protocol::Message::Assistant {
                usage: Some(usage), ..
            } = message_entry.message
        {
            let tokens = crate::features::bottom_bar::context_tokens_from_usage(&usage);
            if tokens > 0 {
                return Some(tokens);
            }
        }
    }
    None
}
