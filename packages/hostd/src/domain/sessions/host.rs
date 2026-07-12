use crate::api::{
    ProtocolError, ServerMessage, SessionSnapshot, SessionSummary, SessionTreeEntry, TurnId,
};
use uuid::Uuid;

use super::types::{HostState, SessionState, now_ms};

impl HostState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_session(&mut self, cwd: impl Into<String>) -> crate::api::CommandResult {
        let cwd = cwd.into();
        let state = SessionState::new(format!("session_{}", Uuid::new_v4()), cwd.clone());
        let session_id = state.session_id.clone();
        self.sessions.insert(session_id.clone(), state);
        crate::api::CommandResult::SessionCreated {
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
            .map(|s| s.summary(None, None, None, None))
            .collect::<Vec<_>>();
        sessions.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        sessions
    }

    pub fn snapshot(&self, session_id: &str) -> Result<SessionSnapshot, ProtocolError> {
        Ok(self.session(session_id)?.snapshot())
    }

    pub fn append_entry(
        &mut self,
        session_id: &str,
        entry: SessionTreeEntry,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        state.current_leaf_id = entry.leaf_target_id().map(str::to_string);
        if let SessionTreeEntry::SessionInfo(session_info) = &entry
            && let Some(name) = &session_info.name
        {
            state.name = Some(name.clone());
        }
        state.entries.push(entry);
        state.seq += 1;
        Ok(())
    }

    pub fn append_task_entry(
        &mut self,
        session_id: &str,
        task_id: &str,
        entry: SessionTreeEntry,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        if let Some(existing) = state
            .entries
            .iter_mut()
            .find(|current| current.id() == entry.id())
        {
            *existing = entry.clone();
            state
                .task_heads
                .insert(task_id.to_string(), entry.id().to_string());
            return Ok(());
        }
        state
            .task_heads
            .insert(task_id.to_string(), entry.id().to_string());
        if state.active_agent_instance_id.as_deref() == Some(task_id) {
            state.current_leaf_id = entry.leaf_target_id().map(str::to_string);
        }
        state.entries.push(entry);
        state.seq += 1;
        Ok(())
    }

    pub fn session_cwd(&self, session_id: &str) -> Result<String, ProtocolError> {
        Ok(self.session(session_id)?.cwd.clone())
    }

    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    pub fn session(&self, session_id: &str) -> Result<&SessionState, ProtocolError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| ProtocolError::SessionNotFound(session_id.to_string()))
    }

    pub fn session_mut(&mut self, session_id: &str) -> Result<&mut SessionState, ProtocolError> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| ProtocolError::SessionNotFound(session_id.to_string()))
    }

    // ---- Turn lifecycle ----

    /// Finalize any in-memory active Turn that cannot have a live Execution after
    /// Session open / rehydrate. Returns lifecycle events that were emitted.
    pub fn finalize_interrupted_turns(
        &mut self,
        session_id: &str,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let turn_id = {
            let state = self.session(session_id)?;
            state.active_turn_id.clone()
        };
        let Some(turn_id) = turn_id else {
            return Ok(Vec::new());
        };
        tracing::info!(
            session_id = %session_id,
            turn_id = %turn_id,
            "finalizing interrupted turn on session open"
        );
        Ok(vec![self.fail_turn(
            session_id,
            &turn_id,
            "turn interrupted: session reopened without a live execution",
        )?])
    }

    pub fn start_turn(
        &mut self,
        session_id: &str,
    ) -> Result<(TurnId, Vec<ServerMessage>), ProtocolError> {
        let state = self.session_mut(session_id)?;
        if let Some(active) = state.active_turn_id.as_ref() {
            tracing::info!(
                session_id = %session_id,
                active_turn_id = %active,
                "refusing start_turn while a turn is still active"
            );
            return Err(ProtocolError::ActiveTurnExists(session_id.to_string()));
        }
        let turn_id = format!("turn_{}", Uuid::new_v4());
        state.active_turn_id = Some(turn_id.clone());
        Ok((turn_id, Vec::new()))
    }

    pub fn complete_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<ServerMessage, ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(ServerMessage::TurnLifecycle(
            crate::api::TurnEvent::Completed {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                total_tasks: 1,
                timestamp: now_ms(),
            },
        ))
    }

    pub fn fail_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
        error: impl Into<String>,
    ) -> Result<ServerMessage, ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(ServerMessage::TurnLifecycle(
            crate::api::TurnEvent::Failed {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                error: error.into(),
                timestamp: now_ms(),
            },
        ))
    }

    pub fn cancel_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<ServerMessage, ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(ServerMessage::TurnLifecycle(
            crate::api::TurnEvent::Cancelled {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                timestamp: now_ms(),
            },
        ))
    }

    pub fn clear_active_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.active_turn_id.as_deref() == Some(turn_id) {
            state.active_turn_id = None;
        }
        Ok(())
    }
}
