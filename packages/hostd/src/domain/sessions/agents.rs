use std::collections::HashMap;

use crate::api::{ProtocolError, ServerMessage, SessionTreeEntry};
use piko_protocol::{AgentViewSnapshot, SequencedServerMessage};

use super::types::{AgentViewState, HostState};

impl HostState {
    pub fn get_agent_list(&self, session_id: &str) -> Vec<crate::api::AgentInfo> {
        if let Ok(state) = self.session(session_id) {
            let mut agents = state.active_agents.values().cloned().collect::<Vec<_>>();
            agents.sort_by(|a, b| {
                let a_depth = agent_instance_depth(&state.active_agents, &a.agent_instance_id);
                let b_depth = agent_instance_depth(&state.active_agents, &b.agent_instance_id);
                a_depth
                    .cmp(&b_depth)
                    .then_with(|| a.agent_instance_id.cmp(&b.agent_instance_id))
            });
            agents
        } else {
            vec![]
        }
    }

    /// Mirror a live `AgentChanged` into session authority so AgentSubscribe can
    /// resolve the instance before any commits arrive.
    pub fn upsert_live_agent(
        &mut self,
        session_id: &str,
        info: crate::api::AgentInfo,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        let agent_instance_id = info.agent_instance_id.clone();
        let agent_id = info.agent_id.clone();
        let created_view = !state.agent_views.contains_key(&agent_instance_id);
        state.active_agents.insert(agent_instance_id.clone(), info);
        if created_view {
            state.agent_views.insert(
                agent_instance_id.clone(),
                AgentViewState::new(agent_instance_id.clone(), agent_id),
            );
            backfill_agent_view_from_entries(state, &agent_instance_id);
        }
        Ok(())
    }

    pub fn set_active_task(
        &mut self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        state.active_agent_instance_id = Some(agent_instance_id.to_string());
        Ok(())
    }

    pub fn append_agent_view_event(
        &mut self,
        session_id: &str,
        agent_instance_id: &str,
        agent_id: &str,
        message: ServerMessage,
    ) -> Result<u64, ProtocolError> {
        let state = self.session_mut(session_id)?;
        let seq = state.next_agent_view_seq;
        state.next_agent_view_seq = state.next_agent_view_seq.saturating_add(1);
        let view = state
            .agent_views
            .entry(agent_instance_id.to_string())
            .or_insert_with(|| {
                AgentViewState::new(agent_instance_id.to_string(), agent_id.to_string())
            });
        view.push(SequencedServerMessage {
            seq,
            message: Box::new(message),
        });
        Ok(seq)
    }

    pub fn ensure_agent_view(
        &mut self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<(), ProtocolError> {
        let state = self.session_mut(session_id)?;
        if state.agent_views.contains_key(agent_instance_id) {
            return Ok(());
        }
        let agent_id = state
            .active_agents
            .get(agent_instance_id)
            .map(|agent| agent.agent_id.clone())
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(format!("unknown agent {agent_instance_id}"))
            })?;
        state.agent_views.insert(
            agent_instance_id.to_string(),
            AgentViewState::new(agent_instance_id.to_string(), agent_id),
        );
        backfill_agent_view_from_entries(state, agent_instance_id);
        Ok(())
    }

    pub fn agent_view_snapshot(
        &mut self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<AgentViewSnapshot, ProtocolError> {
        self.ensure_agent_view(session_id, agent_instance_id)?;
        let state = self.session(session_id)?;
        let view = state.agent_views.get(agent_instance_id).ok_or_else(|| {
            ProtocolError::InvalidCommand(format!("unknown agent {agent_instance_id}"))
        })?;
        let info = state.active_agents.get(agent_instance_id);
        Ok(AgentViewSnapshot {
            agent_instance_id: view.agent_instance_id.clone(),
            agent_id: view.agent_id.clone(),
            parent_agent_instance_id: info.and_then(|info| info.parent_agent_instance_id.clone()),
            status: info.map(|info| info.status.clone()),
            next_seq: view.next_seq,
            events: view.events.iter().cloned().collect(),
        })
    }

    pub fn agent_view_replay(
        &mut self,
        session_id: &str,
        agent_instance_id: &str,
        after_seq: Option<u64>,
    ) -> Result<Vec<SequencedServerMessage>, ProtocolError> {
        self.ensure_agent_view(session_id, agent_instance_id)?;
        let state = self.session(session_id)?;
        let view = state.agent_views.get(agent_instance_id).ok_or_else(|| {
            ProtocolError::InvalidCommand(format!("unknown agent {agent_instance_id}"))
        })?;
        Ok(view.replay_after(after_seq))
    }
}

fn backfill_agent_view_from_entries(
    state: &mut super::types::SessionState,
    agent_instance_id: &str,
) {
    let entries = state.entries.clone();
    for entry in entries {
        let Some((message_agent_id, message)) =
            agent_view_message_from_entry(&state.session_id, &entry, agent_instance_id)
        else {
            continue;
        };
        let seq = state.next_agent_view_seq;
        state.next_agent_view_seq = state.next_agent_view_seq.saturating_add(1);
        if let Some(view) = state.agent_views.get_mut(agent_instance_id) {
            if view.agent_id.is_empty() {
                view.agent_id = message_agent_id;
            }
            view.push(SequencedServerMessage {
                seq,
                message: Box::new(message),
            });
        }
    }
}

fn agent_view_message_from_entry(
    session_id: &str,
    entry: &SessionTreeEntry,
    agent_instance_id: &str,
) -> Option<(String, ServerMessage)> {
    match entry {
        SessionTreeEntry::Message(message) if message.agent_instance_id == agent_instance_id => {
            match &message.message {
                crate::api::Message::User { .. }
                | crate::api::Message::Assistant { .. }
                | crate::api::Message::ToolResult { .. } => Some((
                    message.agent_id.clone(),
                    ServerMessage::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
                        session_id: session_id.to_string(),
                        agent_instance_id: message.agent_instance_id.clone(),
                        agent_id: message.agent_id.clone(),
                        source_turn_id: message.source_turn_id.clone(),
                        message_id: message.id.clone(),
                        transcript_seq: message.transcript_seq,
                        message: message.message.clone(),
                    }),
                )),
                _ => None,
            }
        }
        _ => None,
    }
}

fn agent_instance_depth(
    agents: &HashMap<String, crate::api::AgentInfo>,
    agent_instance_id: &str,
) -> usize {
    let mut depth = 0;
    let mut current = agents
        .get(agent_instance_id)
        .and_then(|agent| agent.parent_agent_instance_id.as_ref());
    while let Some(parent_id) = current {
        depth += 1;
        current = agents
            .get(parent_id)
            .and_then(|agent| agent.parent_agent_instance_id.as_ref());
    }
    depth
}
