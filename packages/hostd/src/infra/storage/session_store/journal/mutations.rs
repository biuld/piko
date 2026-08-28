use piko_protocol::execution::{CommitAck, CommitError, MessageCommit, ModelStepCommit};
use piko_protocol::{Message, SessionTreeEntry};
use piko_session_store::{
    EventData, MessageCommittedV1, ModelStepCommittedV1, TreeEntryRecordedV1, UsageAttribution,
    UsageRecordedV1,
};

use crate::api::MessageEntry;
use crate::ports::storage_types::SessionStorageError;

use super::SessionStore;

impl SessionStore {
    pub fn commit_message(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
    ) -> Result<CommitAck, CommitError> {
        self.with_io(|| self.commit_message_under_lock(commit, agent_spec_id))
    }

    pub(crate) fn commit_message_under_lock(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
    ) -> Result<CommitAck, CommitError> {
        let aggregate = self.aggregate().map_err(Self::commit_error)?;
        if aggregate.session_id.as_deref() != Some(commit.session_id.as_str()) {
            return Err(CommitError::IdentityMismatch);
        }
        let agent = aggregate
            .agents
            .get(&commit.agent_instance_id)
            .ok_or(CommitError::IdentityMismatch)?;
        if agent.identity.agent_spec_id != agent_spec_id {
            return Err(CommitError::IdentityMismatch);
        }
        let tree_parent_entry_id = commit
            .tree_parent_entry_id
            .clone()
            .or_else(|| commit.parent_message_id.clone())
            .or_else(|| {
                aggregate
                    .executions
                    .get(&commit.execution_id)
                    .and_then(|execution| execution.started.tree_base_entry_id.clone())
            })
            .or_else(|| aggregate.selected_tree_entry_id.clone());
        if let Some(existing) = aggregate.messages.get(&commit.message_id) {
            if existing.data.agent_instance_id == commit.agent_instance_id
                && existing.data.agent_parent_message_id == commit.parent_message_id
                && commit.tree_parent_entry_id.as_ref().is_none_or(|parent| {
                    existing.data.tree_parent_entry_id.as_ref() == Some(parent)
                })
                && existing.data.execution_id.as_deref() == Some(commit.execution_id.as_str())
                && existing.data.source_turn_id == commit.source_turn_id
                && existing.data.message == commit.message
            {
                return Ok(CommitAck {
                    session_id: commit.session_id,
                    execution_id: commit.execution_id,
                    agent_instance_id: commit.agent_instance_id,
                    message_id: Some(commit.message_id),
                    revision: existing.revision,
                });
            }
            return Err(CommitError::IdempotencyConflict);
        }
        let transcript_seq = aggregate
            .messages
            .values()
            .filter(|message| message.data.agent_instance_id == commit.agent_instance_id)
            .count() as u64
            + 1;
        let entry = SessionTreeEntry::Message(MessageEntry {
            id: commit.message_id.clone(),
            parent_id: tree_parent_entry_id.clone(),
            timestamp: commit.committed_at.to_string(),
            agent_id: agent_spec_id.to_string(),
            agent_instance_id: commit.agent_instance_id.clone(),
            source_turn_id: commit.source_turn_id.clone().unwrap_or_default(),
            transcript_seq,
            message: commit.message.clone(),
        });
        let mut events = vec![EventData::MessageCommitted(MessageCommittedV1 {
            message_id: commit.message_id.clone(),
            agent_instance_id: commit.agent_instance_id.clone(),
            agent_parent_message_id: commit.parent_message_id.clone(),
            tree_parent_entry_id,
            execution_id: Some(commit.execution_id.clone()),
            source_turn_id: commit.source_turn_id.clone(),
            committed_at: commit.committed_at,
            message: commit.message.clone(),
        })];
        events.push(tree_entry_event(&entry).map_err(Self::commit_error)?);
        if aggregate
            .root
            .as_ref()
            .is_some_and(|root| root.agent_instance_id == commit.agent_instance_id)
        {
            events.push(EventData::BranchSelected {
                selected_tree_entry_id: Some(commit.message_id.clone()),
                root_base_message_id: Some(commit.message_id.clone()),
            });
        }
        if let Message::Assistant {
            provider,
            model,
            usage: Some(usage),
            ..
        } = &commit.message
        {
            events.push(EventData::UsageRecorded(UsageRecordedV1 {
                usage_id: piko_orchd_api::stable_internal_id(
                    "usage",
                    &[&commit.session_id, &commit.message_id],
                ),
                attribution: UsageAttribution {
                    session_id: commit.session_id.clone(),
                    agent_instance_id: commit.agent_instance_id.clone(),
                    turn_id: commit.source_turn_id.clone(),
                    execution_id: commit.execution_id.clone(),
                    model_step_id: commit.message_id.clone(),
                },
                provider: provider.clone(),
                model_id: model.clone(),
                api_surface: None,
                pricing_policy_id: None,
                pricing_revision: None,
                usage: usage.clone(),
                incurred: true,
            }));
        }
        let previous_todo = aggregate.todo_lists.get(&commit.agent_instance_id);
        if let Some(list) = crate::domain::todos::todo_list_from_tool_result(
            &commit.agent_instance_id,
            &commit.message,
            previous_todo,
        ) {
            events.push(EventData::TodoListReplaced {
                agent_instance_id: commit.agent_instance_id.clone(),
                todo_list: (!list.items.is_empty()).then_some(list),
            });
        }
        let commit_id = piko_orchd_api::stable_internal_id(
            "message-commit",
            &[
                &commit.session_id,
                &commit.agent_instance_id,
                &commit.message_id,
            ],
        );
        let revision = self
            .commit_events(&commit_id, commit.committed_at, events)
            .map_err(Self::commit_error)?;
        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision,
        })
    }

    pub fn commit_model_step(
        &self,
        commit: ModelStepCommit,
        agent_spec_id: &str,
    ) -> Result<CommitAck, CommitError> {
        self.with_io(|| self.commit_model_step_under_lock(commit, agent_spec_id))
    }

    pub(crate) fn commit_model_step_under_lock(
        &self,
        commit: ModelStepCommit,
        agent_spec_id: &str,
    ) -> Result<CommitAck, CommitError> {
        let aggregate = self.aggregate().map_err(Self::commit_error)?;
        if aggregate.session_id.as_deref() != Some(commit.session_id.as_str()) {
            return Err(CommitError::IdentityMismatch);
        }
        let agent = aggregate
            .agents
            .get(&commit.agent_instance_id)
            .ok_or(CommitError::IdentityMismatch)?;
        if agent.identity.agent_spec_id != agent_spec_id {
            return Err(CommitError::IdentityMismatch);
        }
        let execution = aggregate
            .executions
            .get(&commit.execution_id)
            .ok_or(CommitError::IdentityMismatch)?;
        if execution.finished_at.is_some() {
            return Err(CommitError::IdentityMismatch);
        }
        if execution.started.run_id != commit.run_id
            || execution.started.agent_instance_id != commit.agent_instance_id
            || execution.started.source_turn_id != commit.source_turn_id
        {
            return Err(CommitError::IdentityMismatch);
        }
        let boundary = commit.boundary();
        let event_data = ModelStepCommittedV1 {
            model_step_id: boundary.model_step_id.clone(),
            step_index: boundary.step_index,
            run_id: boundary.run_id.clone(),
            execution_id: boundary.execution_id.clone(),
            agent_instance_id: boundary.agent_instance_id.clone(),
            source_turn_id: boundary.source_turn_id.clone(),
            assistant_message_id: boundary.assistant_message_id.clone(),
            tool_call_message_ids: boundary.tool_call_message_ids.clone(),
            outcome: boundary.outcome,
            started_at: boundary.started_at,
            finished_at: boundary.finished_at,
        };
        if let Some(existing) = aggregate.model_steps.get(&commit.model_step_id) {
            if existing.data == event_data && step_messages_match_existing(&aggregate, &commit) {
                return Ok(CommitAck {
                    session_id: commit.session_id,
                    execution_id: commit.execution_id,
                    agent_instance_id: commit.agent_instance_id,
                    message_id: Some(commit.assistant.message_id),
                    revision: existing.revision,
                });
            }
            return Err(CommitError::IdempotencyConflict);
        }

        let expected_parent = execution
            .message_head
            .as_ref()
            .or(execution.started.base_message_id.as_ref())
            .cloned();
        if commit.assistant.parent_message_id != expected_parent {
            return Err(CommitError::IdentityMismatch);
        }
        validate_step_message(&commit.assistant, &commit, true)?;
        let mut message_ids = vec![commit.assistant.message_id.clone()];
        message_ids.extend(
            commit
                .tool_calls
                .iter()
                .map(|message| message.message_id.clone()),
        );
        if message_ids
            .iter()
            .any(|id| aggregate.messages.contains_key(id))
            || message_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != message_ids.len()
        {
            return Err(CommitError::IdempotencyConflict);
        }
        for (index, tool_call) in commit.tool_calls.iter().enumerate() {
            validate_step_message(tool_call, &commit, false)?;
            let expected_parent = message_ids[index].clone();
            if tool_call.parent_message_id.as_deref() != Some(expected_parent.as_str()) {
                return Err(CommitError::IdentityMismatch);
            }
        }
        let expected_index = execution.model_step_ids.len() as u32 + 1;
        if commit.step_index != expected_index {
            return Err(CommitError::IdentityMismatch);
        }
        let has_tools = !commit.tool_calls.is_empty();
        if (commit.outcome == piko_protocol::ModelStepOutcome::ToolCalls) != has_tools {
            return Err(CommitError::IdentityMismatch);
        }

        let mut events = Vec::with_capacity(3 + commit.tool_calls.len() * 2);
        let mut parent_message_id = expected_parent;
        let mut tree_parent_entry_id = execution
            .message_head
            .as_ref()
            .or(execution.started.tree_base_entry_id.as_ref())
            .or(aggregate.selected_tree_entry_id.as_ref())
            .cloned();
        let mut transcript_seq = aggregate
            .messages
            .values()
            .filter(|message| message.data.agent_instance_id == commit.agent_instance_id)
            .count() as u64;

        append_step_message(
            &mut events,
            &commit,
            &commit.assistant,
            agent_spec_id,
            &mut parent_message_id,
            &mut tree_parent_entry_id,
            &mut transcript_seq,
        )?;
        if let Message::Assistant {
            provider,
            model,
            usage: Some(usage),
            ..
        } = &commit.assistant.message
        {
            events.push(EventData::UsageRecorded(UsageRecordedV1 {
                usage_id: piko_orchd_api::stable_internal_id(
                    "usage",
                    &[&commit.session_id, &commit.model_step_id],
                ),
                attribution: UsageAttribution {
                    session_id: commit.session_id.clone(),
                    agent_instance_id: commit.agent_instance_id.clone(),
                    turn_id: commit.source_turn_id.clone(),
                    execution_id: commit.execution_id.clone(),
                    model_step_id: commit.model_step_id.clone(),
                },
                provider: provider.clone(),
                model_id: model.clone(),
                api_surface: None,
                pricing_policy_id: None,
                pricing_revision: None,
                usage: usage.clone(),
                incurred: true,
            }));
        }
        for tool_call in &commit.tool_calls {
            append_step_message(
                &mut events,
                &commit,
                tool_call,
                agent_spec_id,
                &mut parent_message_id,
                &mut tree_parent_entry_id,
                &mut transcript_seq,
            )?;
        }
        if aggregate
            .root
            .as_ref()
            .is_some_and(|root| root.agent_instance_id == commit.agent_instance_id)
        {
            let selected = message_ids.last().cloned();
            events.push(EventData::BranchSelected {
                selected_tree_entry_id: selected.clone(),
                root_base_message_id: selected,
            });
        }
        events.push(EventData::ModelStepCommitted(event_data));
        let commit_id = piko_orchd_api::stable_internal_id(
            "model-step-commit",
            &[
                &commit.session_id,
                &commit.execution_id,
                &commit.model_step_id,
            ],
        );
        let revision = self
            .commit_events(&commit_id, commit.finished_at, events)
            .map_err(Self::commit_error)?;
        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(boundary.assistant_message_id),
            revision,
        })
    }
}

fn step_messages_match_existing(
    aggregate: &piko_session_store::SessionAggregate,
    step: &ModelStepCommit,
) -> bool {
    let mut proposed = std::iter::once(&step.assistant).chain(step.tool_calls.iter());
    let message_ids = std::iter::once(step.assistant.message_id.as_str()).chain(
        step.tool_calls
            .iter()
            .map(|message| message.message_id.as_str()),
    );
    message_ids
        .zip(&mut proposed)
        .all(|(message_id, proposed)| {
            aggregate.messages.get(message_id).is_some_and(|stored| {
                stored.data.agent_instance_id == proposed.agent_instance_id
                    && stored.data.agent_parent_message_id == proposed.parent_message_id
                    && proposed.tree_parent_entry_id.as_ref().is_none_or(|parent| {
                        stored.data.tree_parent_entry_id.as_ref() == Some(parent)
                    })
                    && stored.data.execution_id.as_deref() == Some(proposed.execution_id.as_str())
                    && stored.data.source_turn_id == proposed.source_turn_id
                    && stored.data.committed_at == proposed.committed_at
                    && stored.data.message == proposed.message
            })
        })
}

fn validate_step_message(
    message: &MessageCommit,
    step: &ModelStepCommit,
    assistant: bool,
) -> Result<(), CommitError> {
    if message.session_id != step.session_id
        || message.source_turn_id != step.source_turn_id
        || message.execution_id != step.execution_id
        || message.agent_instance_id != step.agent_instance_id
    {
        return Err(CommitError::IdentityMismatch);
    }
    let valid_role = if assistant {
        matches!(&message.message, Message::Assistant { .. })
    } else {
        matches!(&message.message, Message::ToolCall { .. })
    };
    if !valid_role {
        return Err(CommitError::IdentityMismatch);
    }
    Ok(())
}

fn append_step_message(
    events: &mut Vec<EventData>,
    step: &ModelStepCommit,
    message: &MessageCommit,
    agent_spec_id: &str,
    parent_message_id: &mut Option<String>,
    tree_parent_entry_id: &mut Option<String>,
    transcript_seq: &mut u64,
) -> Result<(), CommitError> {
    if message.parent_message_id != *parent_message_id {
        return Err(CommitError::IdentityMismatch);
    }
    let tree_parent = message
        .tree_parent_entry_id
        .clone()
        .or_else(|| tree_parent_entry_id.clone());
    *transcript_seq = (*transcript_seq).saturating_add(1);
    let entry = SessionTreeEntry::Message(MessageEntry {
        id: message.message_id.clone(),
        parent_id: tree_parent.clone(),
        timestamp: message.committed_at.to_string(),
        agent_id: agent_spec_id.to_string(),
        agent_instance_id: step.agent_instance_id.clone(),
        source_turn_id: step.source_turn_id.clone().unwrap_or_default(),
        transcript_seq: *transcript_seq,
        message: message.message.clone(),
    });
    events.push(EventData::MessageCommitted(MessageCommittedV1 {
        message_id: message.message_id.clone(),
        agent_instance_id: step.agent_instance_id.clone(),
        agent_parent_message_id: message.parent_message_id.clone(),
        tree_parent_entry_id: tree_parent.clone(),
        execution_id: Some(step.execution_id.clone()),
        source_turn_id: step.source_turn_id.clone(),
        committed_at: message.committed_at,
        message: message.message.clone(),
    }));
    events.push(tree_entry_event(&entry).map_err(SessionStore::commit_error)?);
    *parent_message_id = Some(message.message_id.clone());
    *tree_parent_entry_id = Some(message.message_id.clone());
    Ok(())
}

pub(crate) fn tree_entry_event(entry: &SessionTreeEntry) -> Result<EventData, SessionStorageError> {
    let payload = serde_json::to_value(entry).map_err(|source| SessionStorageError::Json {
        path: "tree entry".into(),
        source,
    })?;
    let entry_type = payload
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    Ok(EventData::TreeEntryRecorded(TreeEntryRecordedV1 {
        entry_id: entry.id().to_string(),
        parent_entry_id: entry.parent_id().map(str::to_string),
        entry_type,
        timestamp: entry.timestamp().parse().unwrap_or_default(),
        payload,
    }))
}
