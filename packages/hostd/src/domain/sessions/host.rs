use crate::api::{
    ProtocolError, ServerMessage, SessionSnapshot, SessionSummary, SessionTreeEntry, TurnId,
};
use uuid::Uuid;

use super::types::{HostState, SessionState, TurnRecord, now_ms};

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
        agent_instance_id: &str,
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
                .insert(agent_instance_id.to_string(), entry.id().to_string());
            return Ok(());
        }
        state
            .task_heads
            .insert(agent_instance_id.to_string(), entry.id().to_string());
        if state.active_agent_instance_id.as_deref() == Some(agent_instance_id) {
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
        let turns = {
            let state = self.session(session_id)?;
            state.active_turns.values().cloned().collect::<Vec<_>>()
        };
        let mut events = Vec::with_capacity(turns.len());
        for turn_id in turns {
            tracing::info!(
                session_id = %session_id,
                turn_id = %turn_id,
                "finalizing interrupted turn on session open"
            );
            events.push(self.fail_turn(
                session_id,
                &turn_id,
                "turn interrupted: session reopened without a live execution",
            )?);
        }
        Ok(events)
    }

    pub fn start_turn(
        &mut self,
        session_id: &str,
        agent_instance_id: &str,
        message: &str,
    ) -> Result<(TurnId, crate::api::TurnStatus), ProtocolError> {
        let state = self.session_mut(session_id)?;
        let turn_id = format!("turn_{}", Uuid::new_v4());
        // Registration precedes durable delivery to AgentActor. The receipt
        // returned by AgentRuntimeApi is authoritative for Running vs Queued.
        let status = crate::api::TurnStatus::Queued;
        state.turns.insert(
            turn_id.clone(),
            TurnRecord {
                turn_id: turn_id.clone(),
                agent_instance_id: agent_instance_id.to_string(),
                message: message.to_string(),
                status: status.clone(),
            },
        );
        Ok((turn_id, status))
    }

    pub fn apply_turn_input_disposition(
        &mut self,
        session_id: &str,
        turn_id: &str,
        disposition: piko_protocol::InputDisposition,
    ) -> Result<crate::api::TurnStatus, ProtocolError> {
        let state = self.session_mut(session_id)?;
        let turn = state
            .turns
            .get_mut(turn_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("turn not found: {turn_id}")))?;
        match disposition {
            piko_protocol::InputDisposition::Accepted
            | piko_protocol::InputDisposition::Duplicate => {
                turn.status = crate::api::TurnStatus::Running;
                state
                    .active_turns
                    .insert(turn.agent_instance_id.clone(), turn_id.to_string());
            }
            piko_protocol::InputDisposition::Queued => {
                turn.status = crate::api::TurnStatus::Queued;
            }
            piko_protocol::InputDisposition::Overload => {
                turn.status = crate::api::TurnStatus::Failed;
            }
        }
        Ok(turn.status.clone())
    }

    pub fn mark_turn_running(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        let turn = state
            .turns
            .get_mut(turn_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("turn not found: {turn_id}")))?;
        turn.status = crate::api::TurnStatus::Running;
        state
            .active_turns
            .insert(turn.agent_instance_id.clone(), turn_id.to_string());
        Ok(())
    }

    pub fn restore_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
        agent_instance_id: &str,
        message: &str,
        status: crate::api::TurnStatus,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.turns.contains_key(turn_id) {
            return Ok(());
        }
        state.turns.insert(
            turn_id.to_string(),
            TurnRecord {
                turn_id: turn_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                message: message.to_string(),
                status: status.clone(),
            },
        );
        if matches!(
            status,
            crate::api::TurnStatus::Running | crate::api::TurnStatus::Cancelling
        ) {
            state
                .active_turns
                .insert(agent_instance_id.to_string(), turn_id.to_string());
        }
        Ok(())
    }

    pub fn turn(&self, session_id: &str, turn_id: &str) -> Result<&TurnRecord, ProtocolError> {
        self.session(session_id)?
            .turns
            .get(turn_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("turn not found: {turn_id}")))
    }

    pub fn active_turn_for_agent(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Option<&TurnRecord> {
        let state = self.session(session_id).ok()?;
        let turn_id = state.active_turns.get(agent_instance_id)?;
        state.turns.get(turn_id)
    }

    pub fn complete_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<ServerMessage, ProtocolError> {
        let state = self.session_mut(session_id)?;
        let turn = state
            .turns
            .get_mut(turn_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("turn not found: {turn_id}")))?;
        turn.status = crate::api::TurnStatus::Completed;
        let agent_instance_id = turn.agent_instance_id.clone();
        if state
            .active_turns
            .get(&agent_instance_id)
            .map(String::as_str)
            == Some(turn_id)
        {
            state.active_turns.remove(&agent_instance_id);
        }
        Ok(ServerMessage::TurnLifecycle(
            crate::api::TurnEvent::Completed {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                agent_instance_id,
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
        let turn = state
            .turns
            .get_mut(turn_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("turn not found: {turn_id}")))?;
        turn.status = crate::api::TurnStatus::Failed;
        let agent_instance_id = turn.agent_instance_id.clone();
        if state
            .active_turns
            .get(&agent_instance_id)
            .map(String::as_str)
            == Some(turn_id)
        {
            state.active_turns.remove(&agent_instance_id);
        }
        Ok(ServerMessage::TurnLifecycle(
            crate::api::TurnEvent::Failed {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                agent_instance_id,
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
        let turn = state
            .turns
            .get_mut(turn_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("turn not found: {turn_id}")))?;
        let agent_instance_id = turn.agent_instance_id.clone();
        turn.status = crate::api::TurnStatus::Cancelled;
        if state
            .active_turns
            .get(&agent_instance_id)
            .map(String::as_str)
            == Some(turn_id)
        {
            state.active_turns.remove(&agent_instance_id);
        }
        Ok(ServerMessage::TurnLifecycle(
            crate::api::TurnEvent::Cancelled {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                agent_instance_id,
                timestamp: now_ms(),
            },
        ))
    }

    pub fn set_turn_status(
        &mut self,
        session_id: &str,
        turn_id: &str,
        status: crate::api::TurnStatus,
    ) -> Result<(), ProtocolError> {
        let turn = self
            .session_mut(session_id)?
            .turns
            .get_mut(turn_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("turn not found: {turn_id}")))?;
        turn.status = status;
        Ok(())
    }

    pub fn clear_active_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        let agent_instance_id = state
            .turns
            .get(turn_id)
            .map(|turn| turn.agent_instance_id.clone());
        if let Some(agent_instance_id) = agent_instance_id
            && state
                .active_turns
                .get(&agent_instance_id)
                .map(String::as_str)
                == Some(turn_id)
        {
            state.active_turns.remove(&agent_instance_id);
        }
        Ok(())
    }
}
