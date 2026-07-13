use std::collections::HashMap;

use crate::api::{ProtocolError, ServerMessage};
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

    pub fn agent_view_snapshot(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<AgentViewSnapshot, ProtocolError> {
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
        &self,
        session_id: &str,
        agent_instance_id: &str,
        after_seq: Option<u64>,
    ) -> Result<Vec<SequencedServerMessage>, ProtocolError> {
        let state = self.session(session_id)?;
        let view = state.agent_views.get(agent_instance_id).ok_or_else(|| {
            ProtocolError::InvalidCommand(format!("unknown agent {agent_instance_id}"))
        })?;
        Ok(view.replay_after(after_seq))
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
