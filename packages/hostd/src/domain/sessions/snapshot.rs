use crate::api::{SessionSnapshot, TurnSnapshot, TurnStatus};

use super::types::SessionState;

impl SessionState {
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            session_id: self.session_id.clone(),
            cwd: self.cwd.clone(),
            seq: self.seq,
            entries: self.entries.clone(),
            current_leaf_id: self.current_leaf_id.clone(),
            // Pending approvals/interactions are process-local and filled by
            // HostApp::enrich_session_view from the live TurnRunner.
            active_turn: self.active_turn_id.as_ref().map(|turn_id| TurnSnapshot {
                turn_id: turn_id.clone(),
                status: TurnStatus::Running,
                assistant_text: String::new(),
                tool_calls: Vec::new(),
            }),
            pending_approvals: Vec::new(),
            pending_interactions: Vec::new(),
            name: self.name.clone(),
            cumulative_usage: Some(self.cumulative_usage.clone()),
        }
    }
}
