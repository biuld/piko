use crate::api::ServerMessage;

use super::types::{HostState, truncate_preview};

impl HostState {
    pub fn push_steer(
        &mut self,
        session_id: &str,
        task_id: &str,
        message: &str,
    ) -> QueueUpdateEvent {
        let state = self.session_mut(session_id).unwrap();
        state
            .steer_queue
            .push((task_id.to_string(), message.to_string()));
        self.build_queue_update(session_id)
    }

    /// Push a follow-up prompt into the session's follow-up queue.
    pub fn push_follow_up(&mut self, session_id: &str, message: &str) -> QueueUpdateEvent {
        let state = self.session_mut(session_id).unwrap();
        state.follow_up_queue.push(message.to_string());
        self.build_queue_update(session_id)
    }

    /// Push a next-turn prompt into the session's next-turn queue.
    pub fn push_next_turn(&mut self, session_id: &str, message: &str) -> QueueUpdateEvent {
        let state = self.session_mut(session_id).unwrap();
        state.next_turn_queue.push(message.to_string());
        self.build_queue_update(session_id)
    }

    /// Pop and return the next follow-up prompt, if any.
    pub fn drain_next_follow_up(&mut self, session_id: &str) -> Option<String> {
        let state = self.session_mut(session_id).ok()?;
        if state.follow_up_queue.is_empty() {
            None
        } else {
            Some(state.follow_up_queue.remove(0))
        }
    }

    /// Pop and return the next next-turn prompt, if any.
    pub fn drain_next_next_turn(&mut self, session_id: &str) -> Option<String> {
        let state = self.session_mut(session_id).ok()?;
        if state.next_turn_queue.is_empty() {
            None
        } else {
            Some(state.next_turn_queue.remove(0))
        }
    }

    /// Pop and return the next steer item (task_id, message), if any.
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
                follow_up_count: state.follow_up_queue.len() as u32,
                next_turn_count: state.next_turn_queue.len() as u32,
                steer_preview: state.steer_queue.last().map(|(_, m)| truncate_preview(m)),
                follow_up_preview: state.follow_up_queue.last().map(|m| truncate_preview(m)),
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
