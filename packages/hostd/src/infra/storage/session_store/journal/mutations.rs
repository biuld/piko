use piko_protocol::execution::{CommitAck, CommitError, MessageCommit};
use piko_protocol::{Message, SessionTreeEntry};
use piko_session_store::{
    EventData, MessageCommittedV1, TreeEntryRecordedV1, UsageAttribution, UsageRecordedV1,
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
