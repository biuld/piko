use crate::api::{SessionSnapshot, TurnSnapshot};

use super::types::SessionState;

impl SessionState {
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            session_id: self.session_id.clone(),
            cwd: self.cwd.clone(),
            seq: self.seq,
            entries: self.entries.clone(),
            current_leaf_id: self.current_leaf_id.clone(),
            selected_agent_instance_id: self.active_agent_instance_id.clone(),
            // Pending approvals/interactions are process-local and filled by
            // HostApp::enrich_session_view from the live AgentRunRunner.
            active_turns: self
                .active_turns
                .values()
                .filter_map(|turn_id| self.turns.get(turn_id))
                .map(|turn| TurnSnapshot {
                    turn_id: turn.turn_id.clone(),
                    agent_instance_id: turn.agent_instance_id.clone(),
                    status: turn.status.clone(),
                    assistant_text: String::new(),
                    tool_calls: Vec::new(),
                })
                .collect(),
            pending_approvals: Vec::new(),
            pending_interactions: Vec::new(),
            name: self.name.clone(),
            cumulative_usage: Some(self.cumulative_usage.clone()),
        }
    }
}
