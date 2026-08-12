use piko_protocol::AgentRunReport;
use piko_session_store::{EventData, MessageCommittedV1};

use crate::api::{MessageEntry, SessionTreeEntry};
use crate::ports::storage_types::SessionStorageError;

use super::SessionStore;
use super::mutations::tree_entry_event;

impl SessionStore {
    pub fn interrupt_incomplete_agent_executions(&self) -> Result<usize, SessionStorageError> {
        self.with_io(|| {
            let active = self
                .aggregate()?
                .executions
                .into_values()
                .filter(|execution| execution.finished_at.is_none())
                .collect::<Vec<_>>();
            let mut interrupted = 0;
            for execution in active {
                let aggregate = self.aggregate()?;
                let started = &execution.started;
                let agent = aggregate
                    .agents
                    .get(&started.agent_instance_id)
                    .ok_or_else(|| self.invalid("interrupted execution has no agent"))?;
                let now = chrono::Utc::now().timestamp_millis();
                let marker_id = piko_protocol::turn_abort_marker_message_id(&started.execution_id);
                let parent = aggregate
                    .agent_heads
                    .get(&started.agent_instance_id)
                    .cloned();
                let transcript_seq = aggregate
                    .messages
                    .values()
                    .filter(|message| message.data.agent_instance_id == started.agent_instance_id)
                    .count() as u64
                    + 1;
                let marker = piko_protocol::turn_abort_marker(&started.execution_id);
                let entry = SessionTreeEntry::Message(MessageEntry {
                    id: marker_id.clone(),
                    parent_id: parent.clone(),
                    timestamp: now.to_string(),
                    agent_id: agent.identity.agent_spec_id.clone(),
                    agent_instance_id: started.agent_instance_id.clone(),
                    source_turn_id: started.source_turn_id.clone().unwrap_or_default(),
                    transcript_seq,
                    message: marker.clone(),
                });
                let report = AgentRunReport {
                    agent_instance_id: started.agent_instance_id.clone(),
                    report_id: piko_orchd_api::stable_internal_id(
                        "report",
                        &["interrupted", &started.run_id],
                    ),
                    outcome: piko_protocol::ExecutionOutcome::Cancelled {
                        reason: Some("interrupted during session recovery".into()),
                    },
                    summary: "Execution interrupted during session recovery".into(),
                    usage: Default::default(),
                    artifacts: Vec::new(),
                };
                let mut events = vec![
                    EventData::MessageCommitted(MessageCommittedV1 {
                        message_id: marker_id.clone(),
                        agent_instance_id: started.agent_instance_id.clone(),
                        agent_parent_message_id: parent.clone(),
                        tree_parent_entry_id: parent,
                        execution_id: Some(started.execution_id.clone()),
                        source_turn_id: started.source_turn_id.clone(),
                        committed_at: now,
                        message: marker,
                    }),
                    tree_entry_event(&entry)?,
                    EventData::ExecutionFinished {
                        execution_id: started.execution_id.clone(),
                        report,
                        finished_at: now,
                    },
                ];
                if aggregate
                    .root
                    .as_ref()
                    .is_some_and(|root| root.agent_instance_id == started.agent_instance_id)
                {
                    events.push(EventData::BranchSelected {
                        selected_tree_entry_id: Some(marker_id.clone()),
                        root_base_message_id: Some(marker_id),
                    });
                }
                let commit_id = piko_orchd_api::stable_internal_id(
                    "execution-interrupted",
                    &[&aggregate.session_id.unwrap_or_default(), &started.run_id],
                );
                self.commit_events(&commit_id, now, events)?;
                interrupted += 1;
            }
            Ok(interrupted)
        })
    }
}
