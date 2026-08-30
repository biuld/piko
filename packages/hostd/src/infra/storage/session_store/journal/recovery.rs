use std::collections::BTreeSet;

use piko_protocol::{AgentInputDisposition, AgentWorkReport, ContentBlock, Message};
use piko_session_store::{
    AgentInputDispositionChangedV1, EventData, MessageCommittedV1, StoredExecution,
};

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
                let mut parent_message_id = execution
                    .message_head
                    .clone()
                    .or_else(|| started.base_message_id.clone());
                let mut tree_parent_entry_id = execution
                    .message_head
                    .clone()
                    .or_else(|| started.tree_base_entry_id.clone())
                    .or_else(|| aggregate.selected_tree_entry_id.clone());
                let mut transcript_seq = aggregate
                    .messages
                    .values()
                    .filter(|message| message.data.agent_instance_id == started.agent_instance_id)
                    .count() as u64;
                let unresolved = unresolved_tool_calls(&aggregate, &execution)?;
                let pending_steers = aggregate
                    .agent_inputs
                    .values()
                    .filter(|input| {
                        input.input.agent_instance_id == started.agent_instance_id
                            && input.disposition == AgentInputDisposition::PendingSteer
                            && input.root_input_id.as_deref()
                                == aggregate
                                    .agent_inputs
                                    .values()
                                    .find(|root| root.input.request_id == started.request_id)
                                    .and_then(|root| root.root_input_id.as_deref())
                    })
                    .collect::<Vec<_>>();
                let mut events =
                    Vec::with_capacity(4 + unresolved.len() * 2 + pending_steers.len());
                for input in pending_steers {
                    events.push(EventData::AgentInputDispositionChangedV1(
                        AgentInputDispositionChangedV1 {
                            agent_instance_id: started.agent_instance_id.clone(),
                            input_id: input.input.input_id.clone(),
                            disposition: AgentInputDisposition::Cancelled,
                            root_input_id: input.root_input_id.clone(),
                            model_step_id: None,
                            changed_at: now,
                        },
                    ));
                }
                for (tool_call_message_id, tool_call_id, tool_name) in unresolved {
                    let result_id = piko_orchd_api::stable_internal_id(
                        "recovery-tool-result",
                        &[&started.execution_id, &tool_call_message_id],
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
                        started,
                        &agent.identity.agent_spec_id,
                        &result,
                        now,
                        &mut parent_message_id,
                        &mut tree_parent_entry_id,
                        &mut transcript_seq,
                    )?;
                }

                let marker_id = piko_protocol::turn_abort_marker_message_id(&started.execution_id);
                let marker = piko_protocol::turn_abort_marker(&started.execution_id);
                append_recovery_message(
                    &mut events,
                    &marker_id,
                    started,
                    &agent.identity.agent_spec_id,
                    &marker,
                    now,
                    &mut parent_message_id,
                    &mut tree_parent_entry_id,
                    &mut transcript_seq,
                )?;
                let report = AgentWorkReport {
                    agent_instance_id: started.agent_instance_id.clone(),
                    root_input_id: aggregate
                        .agent_inputs
                        .values()
                        .find(|input| input.input.request_id == started.request_id)
                        .and_then(|input| input.root_input_id.clone())
                        .unwrap_or_else(|| started.request_id.clone()),
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
                events.push(EventData::ExecutionFinished {
                    execution_id: started.execution_id.clone(),
                    report,
                    finished_at: now,
                });
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

fn unresolved_tool_calls(
    aggregate: &piko_session_store::SessionAggregate,
    execution: &StoredExecution,
) -> Result<Vec<(String, String, String)>, SessionStorageError> {
    let resolved = aggregate
        .messages
        .values()
        .filter(|message| {
            message.data.execution_id.as_deref() == Some(execution.started.execution_id.as_str())
        })
        .filter_map(|message| match &message.data.message {
            Message::ToolResult { tool_call_id, .. } => Some(tool_call_id.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut unresolved = Vec::new();
    for model_step_id in &execution.model_step_ids {
        let step = aggregate.model_steps.get(model_step_id).ok_or_else(|| {
            SessionStorageError::Invalid {
                path: aggregate.session_id.clone().unwrap_or_default().into(),
                message: format!("execution references unknown model step {model_step_id}"),
            }
        })?;
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
    started: &piko_session_store::ExecutionStartedV1,
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
        agent_instance_id: started.agent_instance_id.clone(),
        source_turn_id: started.source_turn_id.clone().unwrap_or_default(),
        transcript_seq: *transcript_seq,
        message: message.clone(),
    });
    events.push(EventData::MessageCommitted(MessageCommittedV1 {
        message_id: message_id.to_string(),
        agent_instance_id: started.agent_instance_id.clone(),
        agent_parent_message_id: parent_message_id.clone(),
        tree_parent_entry_id: tree_parent_entry_id.clone(),
        execution_id: Some(started.execution_id.clone()),
        source_turn_id: started.source_turn_id.clone(),
        committed_at,
        message: message.clone(),
    }));
    events.push(tree_entry_event(&entry)?);
    *parent_message_id = Some(message_id.to_string());
    *tree_parent_entry_id = Some(message_id.to_string());
    Ok(())
}
