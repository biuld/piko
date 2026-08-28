use std::collections::BTreeSet;

use piko_protocol::{Message, ModelStepOutcome};

use super::SessionAggregate;
use crate::schema::{MessageCommittedV1, ModelStepCommittedV1, RawEvent};
use crate::{Result, StoreError, StoredMessage, StoredModelStep};

impl SessionAggregate {
    pub(super) fn apply_model_step(
        &mut self,
        revision: u64,
        raw: &RawEvent,
        data: ModelStepCommittedV1,
    ) -> Result<()> {
        if data.model_step_id.is_empty()
            || data.run_id.is_empty()
            || data.execution_id.is_empty()
            || data.assistant_message_id.is_empty()
        {
            return Err(StoreError::InvalidEvent(
                "model step requires identity and assistant message".into(),
            ));
        }
        if data.finished_at < data.started_at {
            return Err(StoreError::InvalidEvent(
                "model step finished before it started".into(),
            ));
        }
        if self.model_steps.contains_key(&data.model_step_id) {
            return Err(StoreError::IdempotencyConflict(data.model_step_id));
        }
        let execution = self.executions.get(&data.execution_id).ok_or_else(|| {
            StoreError::InvalidEvent(format!(
                "model step references unknown execution {}",
                data.execution_id
            ))
        })?;
        if execution.finished_at.is_some() {
            return Err(StoreError::InvalidEvent(
                "model step was committed after execution finished".into(),
            ));
        }
        if execution.started.run_id != data.run_id
            || execution.started.agent_instance_id != data.agent_instance_id
            || execution.started.source_turn_id != data.source_turn_id
        {
            return Err(StoreError::InvalidEvent(
                "model step identity does not match execution".into(),
            ));
        }
        let expected_index = execution.model_step_ids.len() as u32 + 1;
        if data.step_index != expected_index {
            return Err(StoreError::InvalidEvent(format!(
                "model step index must be {expected_index}, got {}",
                data.step_index
            )));
        }
        let assistant = self
            .messages
            .get(&data.assistant_message_id)
            .ok_or_else(|| {
                StoreError::InvalidEvent(format!(
                    "model step references unknown assistant message {}",
                    data.assistant_message_id
                ))
            })?;
        if assistant.revision != revision
            || assistant.data.agent_instance_id != data.agent_instance_id
            || assistant.data.execution_id.as_deref() != Some(data.execution_id.as_str())
            || assistant.data.source_turn_id != data.source_turn_id
            || !matches!(&assistant.data.message, Message::Assistant { .. })
        {
            return Err(StoreError::InvalidEvent(
                "model step assistant message revision, identity, or role is invalid".into(),
            ));
        }
        if assistant
            .data
            .agent_parent_message_id
            .as_ref()
            .and_then(|parent_id| self.messages.get(parent_id))
            .is_some_and(|parent| parent.revision == revision)
        {
            return Err(StoreError::InvalidEvent(
                "model step assistant does not extend the pre-commit execution head".into(),
            ));
        }
        if data.tool_call_message_ids.len()
            != data
                .tool_call_message_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
        {
            return Err(StoreError::InvalidEvent(
                "model step contains duplicate tool-call messages".into(),
            ));
        }
        let mut previous_parent = data.assistant_message_id.clone();
        for message_id in &data.tool_call_message_ids {
            let message = self.messages.get(message_id).ok_or_else(|| {
                StoreError::InvalidEvent(format!(
                    "model step references unknown tool-call message {message_id}"
                ))
            })?;
            if message.revision != revision
                || message.data.agent_instance_id != data.agent_instance_id
                || message.data.execution_id.as_deref() != Some(data.execution_id.as_str())
                || message.data.source_turn_id != data.source_turn_id
                || message.data.agent_parent_message_id.as_deref() != Some(previous_parent.as_str())
                || !matches!(&message.data.message, Message::ToolCall { .. })
            {
                return Err(StoreError::InvalidEvent(
                    "model step tool-call revision, identity, ancestry, or role is invalid".into(),
                ));
            }
            previous_parent = message_id.clone();
        }
        if execution.message_head.as_deref() != Some(previous_parent.as_str()) {
            return Err(StoreError::InvalidEvent(
                "model step messages do not end at the execution head".into(),
            ));
        }
        match data.outcome {
            ModelStepOutcome::ToolCalls if data.tool_call_message_ids.is_empty() => {
                return Err(StoreError::InvalidEvent(
                    "tool-call model step has no tool calls".into(),
                ));
            }
            ModelStepOutcome::Completed
            | ModelStepOutcome::Failed
            | ModelStepOutcome::Cancelled
                if !data.tool_call_message_ids.is_empty() =>
            {
                return Err(StoreError::InvalidEvent(
                    "non-tool model step contains tool calls".into(),
                ));
            }
            _ => {}
        }
        self.model_steps.insert(
            data.model_step_id.clone(),
            StoredModelStep {
                revision,
                event_id: raw.event_id.clone(),
                data: data.clone(),
            },
        );
        self.executions
            .get_mut(&data.execution_id)
            .expect("execution validated above")
            .model_step_ids
            .push(data.model_step_id);
        Ok(())
    }

    pub(super) fn apply_message(
        &mut self,
        revision: u64,
        raw: &RawEvent,
        data: MessageCommittedV1,
    ) -> Result<()> {
        if self.messages.contains_key(&data.message_id) {
            return Err(StoreError::IdempotencyConflict(data.message_id));
        }
        if let Some(parent) = &data.agent_parent_message_id {
            let parent = self.messages.get(parent).ok_or_else(|| {
                StoreError::InvalidEvent(format!("unknown agent message parent {parent}"))
            })?;
            if parent.data.agent_instance_id != data.agent_instance_id {
                return Err(StoreError::InvalidEvent(
                    "agent message parent belongs to another agent".into(),
                ));
            }
        }
        if let Some(execution_id) = &data.execution_id
            && let Some(execution) = self.executions.get(execution_id)
        {
            if execution.started.agent_instance_id != data.agent_instance_id {
                return Err(StoreError::InvalidEvent(
                    "execution message belongs to another agent".into(),
                ));
            }
            let expected_parent = execution
                .message_head
                .as_ref()
                .or(execution.started.base_message_id.as_ref());
            if data.agent_parent_message_id.as_ref() != expected_parent {
                return Err(StoreError::InvalidEvent(
                    "execution message does not extend its admitted base".into(),
                ));
            }
            let expected_tree_parent = execution
                .message_head
                .as_ref()
                .or(execution.started.tree_base_entry_id.as_ref());
            if data.tree_parent_entry_id.as_ref() != expected_tree_parent {
                return Err(StoreError::InvalidEvent(
                    "execution message does not extend its admitted tree base".into(),
                ));
            }
        }
        if let Some(parent) = &data.tree_parent_entry_id
            && !self.messages.contains_key(parent)
            && !self.tree_entries.contains_key(parent)
        {
            return Err(StoreError::InvalidEvent(format!(
                "unknown tree parent {parent}"
            )));
        }
        let message_id = data.message_id.clone();
        let execution_id = data.execution_id.clone();
        self.agent_heads
            .insert(data.agent_instance_id.clone(), message_id.clone());
        self.messages.insert(
            message_id.clone(),
            StoredMessage {
                revision,
                event_id: raw.event_id.clone(),
                data,
            },
        );
        if let Some(execution_id) = execution_id
            && let Some(execution) = self.executions.get_mut(&execution_id)
        {
            execution.message_head = Some(message_id);
        }
        Ok(())
    }
}
