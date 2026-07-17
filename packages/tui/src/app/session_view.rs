use super::{AppState, QueueStatus, pending};

impl AppState {
    pub(crate) fn finish_bootstrap_command(&mut self, command_id: &str) {
        let _ = self.session.pending.take(command_id);
        if !self
            .session
            .pending
            .contains_kind(pending::PendingCommandKind::BootstrapConfig)
            && !self
                .session
                .pending
                .contains_kind(pending::PendingCommandKind::BootstrapCatalog)
        {
            self.session.shell_ready = true;
            if !self.session.initializing && self.session.id.is_none() {
                self.agent_panel.mark_hydrated();
            }
        }
    }

    pub(crate) fn begin_session_hydration(&mut self, target_id: Option<String>) {
        if self.session.previous_live_id.is_none() {
            self.session.previous_live_id = self.session.id.clone();
        }
        self.session.opening_id = target_id;
        self.session.initializing = true;
        self.session.active_turns.clear();
        self.timeline.clear();
        self.agent_timelines.clear();
        self.agent_panel.begin_loading();
        self.approvals.clear();
        self.interactions.clear();
        self.queue_status = QueueStatus::default();
        self.tree.document = Default::default();
        self.tree.visible.rows.clear();
    }

    pub(crate) fn clear_session_view(&mut self) {
        self.session.id = None;
        self.session.opening_id = None;
        self.session.previous_live_id = None;
        self.session.initializing = false;
        self.session.active_turns.clear();
        self.timeline.clear();
        self.agent_timelines.clear();
        self.agent_panel.begin_loading();
        if self.session.shell_ready {
            self.agent_panel.mark_hydrated();
        }
        self.approvals.clear();
        self.interactions.clear();
        self.queue_status = QueueStatus::default();
        self.tree.document = Default::default();
        self.tree.visible.rows.clear();
    }
}
