use std::collections::{BTreeMap, BTreeSet};

use piko_protocol::{AgentInboxItem, Message};
use piko_session_store::{SessionAggregate, StoredMessage};

use crate::ports::storage_types::{
    AgentProjection, CommittedMessage, RecoveredAgent, SessionStorageError,
};

use super::SessionStore;

impl SessionStore {
    pub fn usage_summary(
        &self,
        query: &piko_session_store::UsageQuery,
    ) -> Result<piko_session_store::UsageSummary, SessionStorageError> {
        Ok(self.aggregate()?.accounting.summarize(query))
    }

    pub fn ensure_root_agent(
        &self,
        agent_spec_id: &str,
    ) -> Result<piko_protocol::AgentInstanceIdentity, SessionStorageError> {
        let aggregate = self.aggregate()?;
        let root = aggregate
            .root
            .ok_or_else(|| self.invalid("missing root agent"))?;
        if root.agent_spec_id != agent_spec_id {
            return Err(self.invalid("root agent spec mismatch"));
        }
        Ok(root)
    }

    pub fn agent_report_for_turn(
        &self,
        turn_id: &str,
    ) -> Result<Option<piko_protocol::AgentWorkReport>, SessionStorageError> {
        Ok(self
            .aggregate()?
            .agent_inputs
            .values()
            .filter(|input| {
                input.input.input_id == turn_id
                    || input
                        .processing
                        .as_ref()
                        .and_then(|processing| processing.source_turn_id.as_deref())
                        == Some(turn_id)
            })
            .filter_map(|input| input.processing.as_ref().and_then(|p| p.report.clone()))
            .next())
    }

    pub fn agent_instances(&self) -> Result<Vec<AgentProjection>, SessionStorageError> {
        Ok(self.load_projection()?.agents.into_values().collect())
    }

    pub fn agent_inbox(
        &self,
        agent_instance_id: &str,
    ) -> Result<Vec<AgentInboxItem>, SessionStorageError> {
        Ok(self
            .aggregate()?
            .inbox
            .into_values()
            .filter(|item| item.recipient_agent_instance_id == agent_instance_id)
            .collect())
    }

    pub fn agent_transcript(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<Vec<Message>, SessionStorageError> {
        Ok(self
            .load_agent(session_id, agent_instance_id)?
            .transcript
            .into_iter()
            .map(|message| message.message)
            .collect())
    }

    pub fn agent_execution_reports(
        &self,
        agent_instance_id: &str,
    ) -> Result<Vec<piko_orchd_api::RecoveredExecutionReport>, SessionStorageError> {
        Ok(self
            .aggregate()?
            .agent_inputs
            .values()
            .filter(|input| input.input.agent_instance_id == agent_instance_id)
            .filter_map(|input| {
                let processing = input.processing.as_ref()?;
                Some(piko_orchd_api::RecoveredExecutionReport {
                    root_input_id: input.input.input_id.clone(),
                    report: processing.report.clone()?,
                })
            })
            .collect())
    }

    pub fn agent_queued_inputs(
        &self,
        agent_instance_id: &str,
    ) -> Result<Vec<piko_protocol::AgentInput>, SessionStorageError> {
        Ok(self
            .aggregate()?
            .pending_follow_ups(Some(agent_instance_id)))
    }

    pub fn agent_work_snapshot(
        &self,
        agent_instance_id: &str,
    ) -> Result<Option<piko_protocol::AgentWorkSnapshot>, SessionStorageError> {
        Ok(self
            .aggregate()?
            .agent_work_snapshots()
            .remove(agent_instance_id))
    }

    pub fn pending_detached_deliveries(
        &self,
        source_agent_instance_id: &str,
    ) -> Result<Vec<piko_orchd_api::RecoveredDetachedDelivery>, SessionStorageError> {
        let aggregate = self.aggregate()?;
        Ok(aggregate
            .agent_inputs
            .values()
            .filter(|input| input.input.agent_instance_id == source_agent_instance_id)
            .filter_map(|input| {
                let processing = input.processing.as_ref()?;
                let recipient = processing.detached_recipient_agent_instance_id.clone()?;
                let report = processing.report.clone()?;
                if aggregate.inbox.contains_key(&report.report_id) {
                    return None;
                }
                Some(piko_orchd_api::RecoveredDetachedDelivery {
                    recipient_agent_instance_id: recipient,
                    report,
                })
            })
            .collect())
    }

    pub fn load_agent(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<RecoveredAgent, SessionStorageError> {
        let aggregate = self.aggregate()?;
        if aggregate.session_id.as_deref() != Some(session_id) {
            return Err(self.invalid("session identity mismatch"));
        }
        let agent = aggregate
            .agents
            .get(agent_instance_id)
            .ok_or_else(|| self.invalid(format!("unknown agent {agent_instance_id}")))?;
        let head_message_id = aggregate.agent_heads.get(agent_instance_id).cloned();
        let mut current = head_message_id.clone();
        let mut stored = Vec::new();
        let mut visited = BTreeSet::new();
        while let Some(message_id) = current {
            if !visited.insert(message_id.clone()) {
                return Err(self.invalid(format!("message ancestry cycle at {message_id}")));
            }
            let message = aggregate
                .messages
                .get(&message_id)
                .ok_or_else(|| self.invalid(format!("unknown message {message_id}")))?;
            if message.data.agent_instance_id != agent_instance_id {
                return Err(self.invalid("private transcript crosses agents"));
            }
            current = message.data.agent_parent_message_id.clone();
            stored.push(message);
        }
        stored.reverse();
        let seq_by_id = transcript_seqs(&aggregate, agent_instance_id);
        let last_transcript_seq = seq_by_id.len() as u64;
        let transcript = stored
            .into_iter()
            .map(|message| {
                committed_message(
                    message,
                    &agent.identity.agent_spec_id,
                    seq_by_id
                        .get(message.data.message_id.as_str())
                        .copied()
                        .unwrap_or(0),
                )
            })
            .collect();
        Ok(RecoveredAgent {
            session_id: session_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            agent_spec_id: agent.identity.agent_spec_id.clone(),
            transcript,
            head_message_id,
            last_transcript_seq,
        })
    }

    pub fn next_transcript_seq(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<u64, SessionStorageError> {
        Ok(self
            .load_agent(session_id, agent_instance_id)?
            .last_transcript_seq
            + 1)
    }

    pub fn find_committed_message(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Result<Option<CommittedMessage>, SessionStorageError> {
        let aggregate = self.aggregate()?;
        if aggregate.session_id.as_deref() != Some(session_id) {
            return Err(self.invalid("session identity mismatch"));
        }
        let Some(message) = aggregate.messages.get(message_id) else {
            return Ok(None);
        };
        if message.data.agent_instance_id != agent_instance_id {
            return Ok(None);
        }
        let spec_id = &aggregate
            .agents
            .get(agent_instance_id)
            .ok_or_else(|| self.invalid(format!("unknown agent {agent_instance_id}")))?
            .identity
            .agent_spec_id;
        let seq = transcript_seqs(&aggregate, agent_instance_id)
            .get(message.data.message_id.as_str())
            .copied()
            .unwrap_or(0);
        Ok(Some(committed_message(message, spec_id, seq)))
    }

    pub fn list_agents(&self, session_id: &str) -> Result<Vec<String>, SessionStorageError> {
        let aggregate = self.aggregate()?;
        if aggregate.session_id.as_deref() != Some(session_id) {
            return Err(self.invalid("session identity mismatch"));
        }
        Ok(aggregate.agents.keys().cloned().collect())
    }
}

fn transcript_seqs(aggregate: &SessionAggregate, agent_instance_id: &str) -> BTreeMap<String, u64> {
    let mut messages = aggregate
        .messages
        .values()
        .filter(|message| message.data.agent_instance_id == agent_instance_id)
        .collect::<Vec<_>>();
    messages.sort_by_key(|message| message.revision);
    messages
        .into_iter()
        .enumerate()
        .map(|(index, message)| (message.data.message_id.clone(), (index + 1) as u64))
        .collect()
}

fn committed_message(
    stored: &StoredMessage,
    agent_spec_id: &str,
    transcript_seq: u64,
) -> CommittedMessage {
    CommittedMessage {
        id: stored.data.message_id.clone(),
        parent_id: stored.data.agent_parent_message_id.clone(),
        tree_parent_id: stored.data.tree_parent_entry_id.clone(),
        agent_instance_id: stored.data.agent_instance_id.clone(),
        agent_spec_id: agent_spec_id.to_string(),
        root_input_id: stored.data.root_input_id.clone(),
        source_turn_id: stored.data.source_turn_id.clone(),
        transcript_seq,
        timestamp: stored.data.committed_at,
        message: stored.data.message.clone(),
    }
}
