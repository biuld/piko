use std::collections::HashMap;

use crate::api::{
    HostEvent, HostMessage, HostProtocolError, HostSessionSnapshot, SessionId, SessionSummary,
    TurnId, TurnSnapshot, TurnStatus,
};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct HostState {
    sessions: HashMap<SessionId, SessionState>,
}

#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: SessionId,
    pub cwd: String,
    pub seq: u64,
    pub messages: Vec<crate::api::HostMessage>,
    pub active_turn_id: Option<TurnId>,
    pub name: Option<String>,
    pub current_leaf_id: Option<String>,
}

impl SessionState {
    pub fn new(session_id: SessionId, cwd: String) -> Self {
        Self {
            session_id,
            cwd,
            seq: 0,
            messages: Vec::new(),
            active_turn_id: None,
            name: None,
            current_leaf_id: None,
        }
    }

    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            session_id: self.session_id.clone(),
            cwd: self.cwd.clone(),
            seq: self.seq,
            name: self.name.clone(),
        }
    }
}

impl HostState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_session(&mut self, cwd: impl Into<String>) -> HostEvent {
        let cwd = cwd.into();
        let state = SessionState::new(format!("session_{}", Uuid::new_v4()), cwd.clone());
        let session_id = state.session_id.clone();
        self.sessions.insert(session_id.clone(), state);
        HostEvent::SessionCreated {
            session_id,
            cwd,
            timestamp: now_ms(),
        }
    }

    pub fn insert_session(&mut self, state: SessionState) {
        self.sessions.insert(state.session_id.clone(), state);
    }

    pub fn has_session(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    pub fn delete_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
    }

    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        let mut sessions = self
            .sessions
            .values()
            .map(SessionState::summary)
            .collect::<Vec<_>>();
        sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        sessions
    }

    pub fn snapshot(&self, session_id: &str) -> Result<HostSessionSnapshot, HostProtocolError> {
        Ok(self.session(session_id)?.snapshot())
    }

    pub fn add_message(
        &mut self,
        session_id: &str,
        message: HostMessage,
    ) -> Result<(), HostProtocolError> {
        let state = self.session_mut(session_id)?;
        state.current_leaf_id = Some(message.id.clone());
        state.messages.push(message);
        state.seq += 1;
        Ok(())
    }

    pub fn session_cwd(&self, session_id: &str) -> Result<String, HostProtocolError> {
        Ok(self.session(session_id)?.cwd.clone())
    }

    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    pub fn session(&self, session_id: &str) -> Result<&SessionState, HostProtocolError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| HostProtocolError::SessionNotFound(session_id.to_string()))
    }

    pub fn session_mut(
        &mut self,
        session_id: &str,
    ) -> Result<&mut SessionState, HostProtocolError> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| HostProtocolError::SessionNotFound(session_id.to_string()))
    }

    // ---- Turn lifecycle ----

    pub fn start_turn(
        &mut self,
        session_id: &str,
    ) -> Result<(TurnId, Vec<HostEvent>), HostProtocolError> {
        let turn_id = format!("turn_{}", Uuid::new_v4());
        let root_task_id = format!("task_{}", Uuid::new_v4());
        let state = self.session_mut(session_id)?;
        state.active_turn_id = Some(turn_id.clone());
        let event = HostEvent::TurnStarted {
            session_id: session_id.to_string(),
            turn_id: turn_id.clone(),
            root_task_id,
            timestamp: now_ms(),
        };
        Ok((turn_id, vec![event]))
    }

    pub fn complete_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<HostEvent, HostProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(HostEvent::TurnCompleted {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            total_tasks: 1,
            timestamp: now_ms(),
        })
    }

    pub fn fail_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
        error: impl Into<String>,
    ) -> Result<HostEvent, HostProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(HostEvent::TurnFailed {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            error: error.into(),
            timestamp: now_ms(),
        })
    }

    pub fn cancel_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<HostEvent, HostProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(HostEvent::TurnCancelled {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            timestamp: now_ms(),
        })
    }
}

impl SessionState {
    pub fn snapshot(&self) -> HostSessionSnapshot {
        HostSessionSnapshot {
            session_id: self.session_id.clone(),
            cwd: self.cwd.clone(),
            seq: self.seq,
            messages: self.messages.clone(),
            active_turn: self.active_turn_id.as_ref().map(|turn_id| TurnSnapshot {
                turn_id: turn_id.clone(),
                status: TurnStatus::Running,
                assistant_text: String::new(),
                tool_calls: Vec::new(),
            }),
            pending_approvals: Vec::new(),
            name: self.name.clone(),
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
