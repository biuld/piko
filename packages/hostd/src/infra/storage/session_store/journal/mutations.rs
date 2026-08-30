use piko_protocol::execution::{CommitAck, CommitError, MessageCommit};
use piko_protocol::{
    AgentInputDisposition, AgentInputDispositionChange, Message, SessionTreeEntry,
};
use piko_session_store::{
    AgentInputAppliedV1, EventData, MessageCommittedV1, TreeEntryRecordedV1, UsageAttribution,
    UsageRecordedV1,
};

use crate::api::MessageEntry;
use crate::ports::storage_types::SessionStorageError;

use super::SessionStore;

#[path = "model_step.rs"]
mod model_step;

impl SessionStore {
    pub fn commit_message(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
    ) -> Result<CommitAck, CommitError> {
        self.with_io(|| self.commit_message_under_lock(commit, agent_spec_id))
    }

    pub fn commit_steer(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
        change: AgentInputDispositionChange,
    ) -> Result<CommitAck, CommitError> {
        self.with_io(|| self.commit_steer_under_lock(commit, agent_spec_id, change))
    }

    pub(crate) fn commit_message_under_lock(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
    ) -> Result<CommitAck, CommitError> {
        self.commit_message_under_lock_with_steer(commit, agent_spec_id, None)
    }

    /// Commit the user-visible steer message and its causal input transition
    /// in one journal commit. The transition is deliberately coupled here,
    /// rather than inferred from transcript order, so a crash cannot leave a
    /// steer looking delivered while its AgentInput remains pending.
    pub(crate) fn commit_steer_under_lock(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
        change: AgentInputDispositionChange,
    ) -> Result<CommitAck, CommitError> {
        self.commit_message_under_lock_with_steer(commit, agent_spec_id, Some(change))
    }

    fn commit_message_under_lock_with_steer(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
        steer_change: Option<AgentInputDispositionChange>,
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
                if let Some(change) = steer_change.as_ref()
                    && !steer_transition_matches(&aggregate, change)
                {
                    return Err(CommitError::IdempotencyConflict);
                }
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
        if let Some(change) = steer_change.as_ref() {
            validate_steer_commit(&aggregate, &commit, change)?;
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
        let mut events = Vec::with_capacity(6);
        if let Some(change) = steer_change.as_ref() {
            events.push(EventData::AgentInputDispositionChangedV1(
                piko_session_store::AgentInputDispositionChangedV1 {
                    agent_instance_id: change.agent_instance_id.clone(),
                    input_id: change.input_id.clone(),
                    disposition: change.disposition,
                    root_input_id: change.root_input_id.clone(),
                    model_step_id: change.model_step_id.clone(),
                    changed_at: change.changed_at,
                },
            ));
        }
        let applied_input_id = if matches!(&commit.message, Message::User { .. }) {
            steer_change
                .as_ref()
                .map(|change| change.input_id.clone())
                .or_else(|| {
                    aggregate
                        .executions
                        .get(&commit.execution_id)
                        .and_then(|execution| {
                            aggregate
                                .input_by_request
                                .get(&execution.started.request_id)
                                .cloned()
                        })
                })
        } else {
            None
        };
        if let (Message::User { content, .. }, Some(input_id)) = (&commit.message, applied_input_id)
        {
            let input = aggregate
                .agent_inputs
                .get(&input_id)
                .ok_or(CommitError::IdentityMismatch)?;
            if input.input.agent_instance_id != commit.agent_instance_id
                || input.input.content != *content
            {
                return Err(CommitError::IdentityMismatch);
            }
            events.push(EventData::AgentInputAppliedV1(AgentInputAppliedV1 {
                input_id,
                message_id: commit.message_id.clone(),
                agent_instance_id: commit.agent_instance_id.clone(),
                agent_parent_message_id: commit.parent_message_id.clone(),
                tree_parent_entry_id,
                execution_id: commit.execution_id.clone(),
                source_turn_id: commit.source_turn_id.clone(),
                committed_at: commit.committed_at,
            }));
        } else {
            events.push(EventData::MessageCommitted(MessageCommittedV1 {
                message_id: commit.message_id.clone(),
                agent_instance_id: commit.agent_instance_id.clone(),
                agent_parent_message_id: commit.parent_message_id.clone(),
                tree_parent_entry_id,
                execution_id: Some(commit.execution_id.clone()),
                source_turn_id: commit.source_turn_id.clone(),
                committed_at: commit.committed_at,
                message: commit.message.clone(),
            }));
            events.push(tree_entry_event(&entry).map_err(Self::commit_error)?);
        }
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
        let commit_id = if let Some(change) = steer_change.as_ref() {
            piko_orchd_api::stable_internal_id(
                "steer-commit",
                &[
                    &commit.session_id,
                    &commit.agent_instance_id,
                    &change.input_id,
                    &commit.message_id,
                ],
            )
        } else {
            piko_orchd_api::stable_internal_id(
                "message-commit",
                &[
                    &commit.session_id,
                    &commit.agent_instance_id,
                    &commit.message_id,
                ],
            )
        };
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
}

fn validate_steer_commit(
    aggregate: &piko_session_store::SessionAggregate,
    message: &MessageCommit,
    change: &AgentInputDispositionChange,
) -> Result<(), CommitError> {
    if change.disposition != AgentInputDisposition::AppliedToStep
        || message.agent_instance_id != change.agent_instance_id
        || !matches!(&message.message, Message::User { .. })
    {
        return Err(CommitError::IdentityMismatch);
    }
    let execution = aggregate
        .executions
        .get(&message.execution_id)
        .ok_or(CommitError::IdentityMismatch)?;
    if execution.finished_at.is_some()
        || execution.started.agent_instance_id != message.agent_instance_id
        || execution.started.source_turn_id != message.source_turn_id
    {
        return Err(CommitError::IdentityMismatch);
    }
    let root_input_id = change
        .root_input_id
        .as_deref()
        .ok_or(CommitError::IdentityMismatch)?;
    let root = aggregate
        .agent_inputs
        .get(root_input_id)
        .ok_or(CommitError::IdentityMismatch)?;
    if root.input.agent_instance_id != message.agent_instance_id
        || root.disposition != AgentInputDisposition::AppliedAsRoot
        || root.root_input_id.as_deref() != Some(root_input_id)
        || root.input.request_id != execution.started.request_id
    {
        return Err(CommitError::IdentityMismatch);
    }
    let input = aggregate
        .agent_inputs
        .get(&change.input_id)
        .ok_or(CommitError::IdentityMismatch)?;
    if input.input.agent_instance_id != message.agent_instance_id
        || input.disposition != AgentInputDisposition::PendingSteer
        || input.root_input_id.as_deref() != Some(root_input_id)
    {
        return Err(CommitError::IdentityMismatch);
    }
    let expected_step_id = format!(
        "{}:step_{}",
        execution.started.execution_id,
        execution.model_step_ids.len().saturating_add(1)
    );
    if change.model_step_id.as_deref() != Some(expected_step_id.as_str())
        || message.parent_message_id != execution.message_head
    {
        return Err(CommitError::IdentityMismatch);
    }
    Ok(())
}

fn steer_transition_matches(
    aggregate: &piko_session_store::SessionAggregate,
    change: &AgentInputDispositionChange,
) -> bool {
    aggregate
        .agent_inputs
        .get(&change.input_id)
        .is_some_and(|input| {
            input.input.agent_instance_id == change.agent_instance_id
                && input.disposition == AgentInputDisposition::AppliedToStep
                && input.root_input_id == change.root_input_id
                && input.model_step_id == change.model_step_id
        })
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
