use crate::api::SessionSnapshot;

use super::types::SessionState;

impl SessionState {
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            session_id: self.session_id.clone(),
            cwd: self.cwd.clone(),
            seq: self.seq,
            entries: self.entries.clone(),
            model_steps: Vec::new(),
            current_leaf_id: self.current_leaf_id.clone(),
            selected_agent_instance_id: self.active_agent_instance_id.clone(),
            agent_work: Vec::new(),
            pending_approvals: Vec::new(),
            pending_interactions: Vec::new(),
            name: self.name.clone(),
            cumulative_usage: Some(self.cumulative_usage.clone()),
            agent_usage: self.agent_usage_for_snapshot(),
            todo_lists: self.todo_lists_for_snapshot(),
        }
    }
}
