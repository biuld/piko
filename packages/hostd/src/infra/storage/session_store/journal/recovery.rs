use std::collections::BTreeSet;

use piko_protocol::{AgentInputDisposition, AgentWorkReport, ContentBlock, Message};
use piko_session_store::{
    AgentInputDispositionChangedV1, EventData, MessageCommittedV1, StoredAgentInput,
    StoredRootProcessing,
};

use crate::api::{MessageEntry, SessionTreeEntry};
use crate::ports::storage_types::SessionStorageError;

use super::SessionStore;
use super::mutations::tree_entry_event;

impl SessionStore {
    pub fn interrupt_incomplete_agent_work(&self) -> Result<usize, SessionStorageError> {
        self.with_io(|| {
            let active = self
                .aggregate()?
                .agent_inputs
                .into_values()
                .filter(|input| input.has_unfinished_processing())
                .collect::<Vec<_>>();
            let mut interrupted = 0;
            for input in active {
                let aggregate = self.aggregate()?;
                let processing = input
                    .processing
                    .as_ref()
                    .expect("active work has processing facts");
                let started = StartedWorkRef {
                    input: &input,
                    processing,
                };
                let agent = aggregate
                    .agents
                    .get(&input.input.agent_instance_id)
                    .ok_or_else(|| self.invalid("interrupted work has no agent"))?;
                let now = chrono::Utc::now().timestamp_millis();
                let head = aggregate.work_message_head(started.root_input_id());
                let mut parent_message_id = head
                    .map(str::to_string)
                    .or_else(|| processing.base_message_id.clone());
                let mut tree_parent_entry_id = head
                    .map(str::to_string)
                    .or_else(|| processing.tree_base_entry_id.clone())
                    .or_else(|| aggregate.selected_tree_entry_id.clone());
                let mut transcript_seq = aggregate
                    .messages
                    .values()
                    .filter(|message| {
                        message.data.agent_instance_id == input.input.agent_instance_id
                    })
                    .count() as u64;
                let unresolved = unresolved_tool_calls(&aggregate, &started)?;
                let pending_steers = aggregate
                    .agent_inputs
                    .values()
                    .filter(|steer| {
                        steer.input.agent_instance_id == input.input.agent_instance_id
                            && steer.disposition == AgentInputDisposition::PendingSteer
                            && steer.root_input_id.as_deref() == Some(&input.input.input_id)
                    })
                    .collect::<Vec<_>>();
                let mut events =
                    Vec::with_capacity(4 + unresolved.len() * 2 + pending_steers.len());
                for steer in pending_steers {
                    events.push(EventData::AgentInputDispositionChangedV1(
                        AgentInputDispositionChangedV1 {
                            agent_instance_id: input.input.agent_instance_id.clone(),
                            input_id: steer.input.input_id.clone(),
                            disposition: AgentInputDisposition::Cancelled,
                            root_input_id: steer.root_input_id.clone(),
                            model_step_id: None,
                            changed_at: now,
                        },
                    ));
                }
                for (tool_call_message_id, tool_call_id, tool_name) in unresolved {
                    let result_id = piko_orchd_api::stable_internal_id(
                        "recovery-tool-result",
                        &[started.root_input_id(), &tool_call_message_id],
                    );
                    let result = Message::ToolResult {
                        tool_call_id,
                        tool_name: Some(tool_name),
                        content: vec![ContentBlock::Text {
                            text: "Task cancelled".into(),
                        }],
                        details: Some(serde_json::json!({
                            "code": "aborted",
                            "message": "Task cancelled",
                            "retryable": false,
                        })),
                        is_error: Some(true),
                        timestamp: Some(now),
                    };
                    append_recovery_message(
                        &mut events,
                        &result_id,
                        &started,
                        &agent.identity.agent_spec_id,
                        &result,
                        now,
                        &mut parent_message_id,
                        &mut tree_parent_entry_id,
                        &mut transcript_seq,
                    )?;
                }

                let marker_id =
                    piko_protocol::agent_work_abort_marker_message_id(started.root_input_id());
                let marker = piko_protocol::agent_work_abort_marker(started.root_input_id());
                append_recovery_message(
                    &mut events,
                    &marker_id,
                    &started,
                    &agent.identity.agent_spec_id,
                    &marker,
                    now,
                    &mut parent_message_id,
                    &mut tree_parent_entry_id,
                    &mut transcript_seq,
                )?;
                let report = AgentWorkReport {
                    agent_instance_id: input.input.agent_instance_id.clone(),
                    root_input_id: input.input.input_id.clone(),
                    report_id: piko_orchd_api::stable_internal_id(
                        "report",
                        &["interrupted", &input.input.input_id],
                    ),
                    outcome: piko_protocol::AgentWorkOutcome::Cancelled {
                        reason: Some("interrupted during session recovery".into()),
                    },
                    summary: "Agent work interrupted during session recovery".into(),
                    usage: Default::default(),
                    artifacts: Vec::new(),
                };
                events.push(EventData::AgentInputProcessingFinishedV1(
                    piko_session_store::AgentInputProcessingFinishedV1 {
                        agent_instance_id: input.input.agent_instance_id.clone(),
                        root_input_id: input.input.input_id.clone(),
                        report,
                        finished_at: now,
                    },
                ));
                if aggregate
                    .root
                    .as_ref()
                    .is_some_and(|root| root.agent_instance_id == input.input.agent_instance_id)
                {
                    events.push(EventData::BranchSelected {
                        selected_tree_entry_id: Some(marker_id.clone()),
                        root_base_message_id: Some(marker_id),
                    });
                }
                let commit_id = piko_orchd_api::stable_internal_id(
                    "work-interrupted",
                    &[
                        &aggregate.session_id.unwrap_or_default(),
                        &input.input.input_id,
                    ],
                );
                self.commit_events(&commit_id, now, events)?;
                interrupted += 1;
            }
            Ok(interrupted)
        })
    }
}

/// Borrowed view of one unfinished root AgentInput and its processing facts.
struct StartedWorkRef<'a> {
    input: &'a StoredAgentInput,
    processing: &'a StoredRootProcessing,
}

impl StartedWorkRef<'_> {
    fn root_input_id(&self) -> &str {
        &self.input.input.input_id
    }
}

fn unresolved_tool_calls(
    aggregate: &piko_session_store::SessionAggregate,
    work: &StartedWorkRef<'_>,
) -> Result<Vec<(String, String, String)>, SessionStorageError> {
    let root_input_id = work.root_input_id();
    let resolved = aggregate
        .messages
        .values()
        .filter(|message| message.data.root_input_id.as_deref() == Some(root_input_id))
        .filter_map(|message| match &message.data.message {
            Message::ToolResult { tool_call_id, .. } => Some(tool_call_id.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut unresolved = Vec::new();
    for step in aggregate
        .model_steps
        .values()
        .filter(|step| step.data.root_input_id == root_input_id)
    {
        for message_id in &step.data.tool_call_message_ids {
            let message =
                aggregate
                    .messages
                    .get(message_id)
                    .ok_or_else(|| SessionStorageError::Invalid {
                        path: aggregate.session_id.clone().unwrap_or_default().into(),
                        message: format!(
                            "model step references unknown tool call message {message_id}"
                        ),
                    })?;
            let Message::ToolCall { id, name, .. } = &message.data.message else {
                return Err(SessionStorageError::Invalid {
                    path: aggregate.session_id.clone().unwrap_or_default().into(),
                    message: format!("model step message {message_id} is not a tool call"),
                });
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            if !resolved.contains(id) {
                unresolved.push((message_id.clone(), id.clone(), name.clone()));
            }
        }
    }
    Ok(unresolved)
}

#[allow(clippy::too_many_arguments)]
fn append_recovery_message(
    events: &mut Vec<EventData>,
    message_id: &str,
    work: &StartedWorkRef<'_>,
    agent_spec_id: &str,
    message: &Message,
    committed_at: i64,
    parent_message_id: &mut Option<String>,
    tree_parent_entry_id: &mut Option<String>,
    transcript_seq: &mut u64,
) -> Result<(), SessionStorageError> {
    *transcript_seq = transcript_seq.saturating_add(1);
    let entry = SessionTreeEntry::Message(MessageEntry {
        id: message_id.to_string(),
        parent_id: tree_parent_entry_id.clone(),
        timestamp: committed_at.to_string(),
        agent_id: agent_spec_id.to_string(),
        agent_instance_id: work.input.input.agent_instance_id.clone(),
        root_input_id: work
            .processing
            .root_input_id
            .clone()
            .unwrap_or_else(|| work.root_input_id().to_string()),
        transcript_seq: *transcript_seq,
        message: message.clone(),
    });
    events.push(EventData::MessageCommitted(MessageCommittedV1 {
        message_id: message_id.to_string(),
        agent_instance_id: work.input.input.agent_instance_id.clone(),
        agent_parent_message_id: parent_message_id.clone(),
        tree_parent_entry_id: tree_parent_entry_id.clone(),
        root_input_id: work
            .processing
            .root_input_id
            .clone()
            .or_else(|| Some(work.root_input_id().to_string())),
        committed_at,
        message: message.clone(),
    }));
    events.push(tree_entry_event(&entry)?);
    *parent_message_id = Some(message_id.to_string());
    *tree_parent_entry_id = Some(message_id.to_string());
    Ok(())
}
