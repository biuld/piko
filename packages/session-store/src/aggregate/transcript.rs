use std::collections::BTreeSet;

use piko_protocol::{Message, ModelStepOutcome};

use super::SessionAggregate;
use crate::schema::{AgentInputAppliedV1, MessageCommittedV1, ModelStepCommittedV1, RawEvent};
use crate::{Result, StoreError, StoredMessage, StoredModelStep};

impl SessionAggregate {
    pub(super) fn apply_agent_input(
        &mut self,
        revision: u64,
        raw: &RawEvent,
        data: AgentInputAppliedV1,
    ) -> Result<()> {
        let input = self.agent_inputs.get(&data.input_id).ok_or_else(|| {
            StoreError::InvalidEvent(format!("unknown applied input {}", data.input_id))
        })?;
        if input.input.agent_instance_id != data.agent_instance_id {
            return Err(StoreError::InvalidEvent(
                "applied input belongs to another agent".into(),
            ));
        }
        if input.applied_message_id.as_deref() == Some(data.message_id.as_str()) {
            return Ok(());
        }
        if input.applied_message_id.is_some() {
            return Err(StoreError::IdempotencyConflict(data.input_id));
        }
        if !matches!(
            input.disposition,
            piko_protocol::AgentInputDisposition::AppliedAsRoot
                | piko_protocol::AgentInputDisposition::AppliedToStep
        ) {
            return Err(StoreError::InvalidEvent(
                "input must be applied before entering the transcript".into(),
            ));
        }
        let content = input.input.content.clone();
        self.apply_message(
            revision,
            raw,
            MessageCommittedV1 {
                message_id: data.message_id.clone(),
                agent_instance_id: data.agent_instance_id,
                agent_parent_message_id: data.agent_parent_message_id,
                tree_parent_entry_id: data.tree_parent_entry_id,
                execution_id: Some(data.execution_id),
                source_turn_id: data.source_turn_id,
                committed_at: data.committed_at,
                message: Message::User {
                    content,
                    timestamp: Some(data.committed_at),
                },
            },
        )?;
        self.agent_inputs
            .get_mut(&data.input_id)
            .expect("input validated above")
            .applied_message_id = Some(data.message_id);
        Ok(())
    }

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
        let execution_id = data.execution_id.clone();
        let (root, processing) = self.work_by_execution_id(&execution_id).ok_or_else(|| {
            StoreError::InvalidEvent(format!(
                "model step references unknown work execution {execution_id}"
            ))
        })?;
        if processing.finished_at.is_some() {
            return Err(StoreError::InvalidEvent(
                "model step was committed after work finished".into(),
            ));
        }
        if processing.run_id.as_deref() != Some(data.run_id.as_str())
            || root.input.agent_instance_id != data.agent_instance_id
            || processing.source_turn_id != data.source_turn_id
        {
            return Err(StoreError::InvalidEvent(
                "model step identity does not match the root work".into(),
            ));
        }
        if root.disposition != piko_protocol::AgentInputDisposition::AppliedAsRoot
            || root.root_input_id.as_deref() != Some(root.input.input_id.as_str())
        {
            return Err(StoreError::InvalidEvent(
                "model step does not reference an applied root input".into(),
            ));
        }
        let root_input_id = root.input.input_id.clone();
        let expected_index = self.work_model_step_count(&execution_id) as u32 + 1;
        if data.step_index != expected_index {
            return Err(StoreError::InvalidEvent(format!(
                "model step index must be {expected_index}, got {}",
                data.step_index
            )));
        }
        for input in self.agent_inputs.values().filter(|input| {
            input.disposition == piko_protocol::AgentInputDisposition::AppliedToStep
                && input.model_step_id.as_deref() == Some(data.model_step_id.as_str())
        }) {
            let Some(input_root_id) = input.root_input_id.as_deref() else {
                return Err(StoreError::InvalidEvent(
                    "applied steer is missing its causal root".into(),
                ));
            };
            if input.input.agent_instance_id != data.agent_instance_id
                || input_root_id != root_input_id
            {
                return Err(StoreError::InvalidEvent(
                    "applied steer does not match the model step root".into(),
                ));
            }
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
        let head = self.work_message_head(&execution_id).map(str::to_string);
        if head.as_deref() != Some(previous_parent.as_str()) {
            return Err(StoreError::InvalidEvent(
                "model step messages do not end at the work transcript head".into(),
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
            && let Some((root, processing)) = self.work_by_execution_id(execution_id)
        {
            if root.input.agent_instance_id != data.agent_instance_id {
                return Err(StoreError::InvalidEvent(
                    "work message belongs to another agent".into(),
                ));
            }
            let expected_parent = self
                .work_message_head(execution_id)
                .map(str::to_string)
                .or_else(|| processing.base_message_id.clone());
            if data.agent_parent_message_id != expected_parent {
                return Err(StoreError::InvalidEvent(
                    "work message does not extend its admitted base".into(),
                ));
            }
            let expected_tree_parent = self
                .work_message_head(execution_id)
                .map(str::to_string)
                .or_else(|| processing.tree_base_entry_id.clone());
            if data.tree_parent_entry_id != expected_tree_parent {
                return Err(StoreError::InvalidEvent(
                    "work message does not extend its admitted tree base".into(),
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
        self.agent_heads
            .insert(data.agent_instance_id.clone(), message_id.clone());
        self.messages.insert(
            message_id,
            StoredMessage {
                revision,
                event_id: raw.event_id.clone(),
                data,
            },
        );
        Ok(())
    }
}
