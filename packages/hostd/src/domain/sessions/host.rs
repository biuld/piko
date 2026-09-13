use crate::api::{ProtocolError, SessionSnapshot, SessionSummary, SessionTreeEntry};
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
        if entry.advances_selected_branch() {
            state.current_leaf_id = Some(entry.id().to_string());
        }
        if let SessionTreeEntry::SessionInfo(session_info) = &entry
            && let Some(name) = &session_info.name
        {
            state.name = Some(name.clone());
        }
        state.entries.push(entry);
        state.seq += 1;
        Ok(())
    }

    pub fn select_branch(
        &mut self,
        session_id: &str,
        target_id: Option<String>,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        if let Some(id) = target_id.as_deref()
            && !state.entries.iter().any(|entry| entry.id() == id)
        {
            return Err(ProtocolError::InvalidCommand(format!(
                "unknown tree entry: {id}"
            )));
        }
        state.current_leaf_id = target_id;
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
        // Late-observed durable facts (e.g. a world-state message committed at
        // assembly but projected after the turn's assistant message) must never
        // drag the live cursor back onto an older message. The durable journal
        // already advanced past them; only move the cursor forward.
        if state.active_agent_instance_id.as_deref() == Some(agent_instance_id)
            && entry.advances_selected_branch()
            && !entry_is_ancestor_of_cursor(state, entry.id())
        {
            state.current_leaf_id = Some(entry.id().to_string());
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
}

/// Whether `entry_id` already lies on the parent ancestry of the current
/// cursor. Cycles and missing parents terminate the walk safely.
fn entry_is_ancestor_of_cursor(state: &SessionState, entry_id: &str) -> bool {
    let Some(start) = state.current_leaf_id.clone() else {
        return false;
    };
    let mut current = Some(start);
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = current {
        if id == entry_id {
            return true;
        }
        if !visited.insert(id.clone()) {
            return false;
        }
        current = state
            .entries
            .iter()
            .find(|entry| entry.id() == id)
            .and_then(|entry| entry.parent_id().map(str::to_string));
    }
    false
}
