use crate::api::ServerMessage;

use super::types::{HostState, truncate_preview};

impl HostState {
    pub fn push_steer(
        &mut self,
        session_id: &str,
        agent_instance_id: &str,
        message: &str,
    ) -> QueueUpdateEvent {
        let state = self.session_mut(session_id).unwrap();
        state
            .steer_queue
            .push((agent_instance_id.to_string(), message.to_string()));
        self.build_queue_update(session_id)
    }

    /// Pop and return the next steer item (agent_instance_id, message), if any.
    pub fn drain_next_steer(&mut self, session_id: &str) -> Option<(String, String)> {
        let state = self.session_mut(session_id).ok()?;
        if state.steer_queue.is_empty() {
            None
        } else {
            Some(state.steer_queue.remove(0))
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
