use crate::api::ServerMessage;

use super::types::{HostState, truncate_preview};

impl HostState {
    pub fn push_steer(
        &mut self,
        session_id: &str,
        agent_instance_id: &str,
        message: &str,
    ) -> QueueUpdateEvent {
        if let Ok(state) = self.session_mut(session_id) {
            state
                .steer_queue
                .push((agent_instance_id.to_string(), message.to_string()));
        }
        self.build_queue_update(session_id)
    }

    /// Drop in-flight steers for one agent. Called when that agent's active
    /// turn becomes terminal so `QueueEvent` counts cannot grow forever.
    pub fn clear_steers_for_agent(&mut self, session_id: &str, agent_instance_id: &str) {
        if let Ok(state) = self.session_mut(session_id) {
            state
                .steer_queue
                .retain(|(owner, _)| owner != agent_instance_id);
        }
    }

    /// Build a QueueUpdate ServerMessage for the given session.
    pub fn build_queue_update(&self, session_id: &str) -> QueueUpdateEvent {
        if let Some(state) = self.sessions.get(session_id) {
            QueueUpdateEvent {
                session_id: session_id.to_string(),
                steer_count: state.steer_queue.len() as u32,
                follow_up_count: 0,
                next_turn_count: 0,
                steer_preview: state.steer_queue.last().map(|(_, m)| truncate_preview(m)),
                follow_up_preview: None,
            }
        } else {
            QueueUpdateEvent::default()
        }
    }
}

/// Intermediate type for building QueueUpdate. Converted to ServerMessage by caller.
#[derive(Debug, Default)]
pub struct QueueUpdateEvent {
    pub session_id: String,
    pub steer_count: u32,
    pub follow_up_count: u32,
    pub next_turn_count: u32,
    pub steer_preview: Option<String>,
    pub follow_up_preview: Option<String>,
}

impl From<QueueUpdateEvent> for ServerMessage {
    fn from(q: QueueUpdateEvent) -> Self {
        ServerMessage::Queue(crate::api::QueueEvent::Updated {
            session_id: q.session_id,
            steer_count: q.steer_count,
            follow_up_count: q.follow_up_count,
            next_turn_count: q.next_turn_count,
            steer_preview: q.steer_preview,
            follow_up_preview: q.follow_up_preview,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::sessions::HostState;

    fn session() -> (HostState, String) {
        let mut state = HostState::new();
        let crate::api::CommandResult::SessionCreated { session_id, .. } =
            state.create_session("/tmp")
        else {
            panic!("session create");
        };
        (state, session_id)
    }

    #[test]
    fn push_steer_counts_per_session() {
        let (mut state, session_id) = session();
        state.push_steer(&session_id, "agent-a", "one");
        let ev = state.push_steer(&session_id, "agent-a", "two");
        assert_eq!(ev.steer_count, 2);
        assert_eq!(ev.steer_preview.as_deref(), Some("two"));
    }

    #[test]
    fn ending_active_turn_clears_that_agents_steers() {
        let (mut state, session_id) = session();
        let (turn_id, _) = state.start_turn(&session_id, "agent-a", "go").unwrap();
        state.mark_turn_running(&session_id, &turn_id).unwrap();
        state.push_steer(&session_id, "agent-a", "left");
        state.push_steer(&session_id, "agent-b", "stay");
        state.cancel_turn(&session_id, &turn_id).unwrap();
        let ev = state.build_queue_update(&session_id);
        assert_eq!(ev.steer_count, 1);
        assert_eq!(ev.steer_preview.as_deref(), Some("stay"));
    }

    #[test]
    fn clear_steers_for_agent_leaves_other_agents() {
        let (mut state, session_id) = session();
        state.push_steer(&session_id, "agent-a", "a");
        state.push_steer(&session_id, "agent-b", "b");
        state.clear_steers_for_agent(&session_id, "agent-a");
        let ev = state.build_queue_update(&session_id);
        assert_eq!(ev.steer_count, 1);
        assert_eq!(ev.steer_preview.as_deref(), Some("b"));
    }
}
