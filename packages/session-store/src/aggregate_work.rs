//! AgentInput reducers and the derived work indexes/snapshot.

use piko_protocol::AgentInputDisposition;

use crate::aggregate::SessionAggregate;
use crate::projection::StoredRootProcessing;
use crate::schema::{
    AgentInputAdmittedV1, AgentInputDispositionChangedV1, AgentInputProcessingFinishedV1,
    AgentInputProcessingStartedV1, RawEvent,
};
use crate::{Result, StoreError, StoredAgentInput};

impl SessionAggregate {
    pub(crate) fn apply_agent_input_admitted(
        &mut self,
        revision: u64,
        raw: &RawEvent,
        data: AgentInputAdmittedV1,
    ) -> Result<()> {
        let disposition = canonical_disposition(data.disposition)?;
        let input = data.input;
        self.ensure_agent(&input.agent_instance_id)?;
        if Some(input.session_id.as_str()) != self.session_id.as_deref() {
            return Err(StoreError::InvalidEvent(
                "input belongs to another session".into(),
            ));
        }
        if input.input_id.is_empty() || input.request_id.is_empty() {
            return Err(StoreError::InvalidEvent(
                "input requires input and request identities".into(),
            ));
        }
        let root_input_id = match disposition {
            AgentInputDisposition::AppliedAsRoot => {
                if data
                    .root_input_id
                    .as_deref()
                    .is_some_and(|id| id != input.input_id)
                {
                    return Err(StoreError::InvalidEvent(
                        "root input must identify itself".into(),
                    ));
                }
                Some(input.input_id.clone())
            }
            AgentInputDisposition::PendingSteer => {
                let target = data.root_input_id.as_deref().ok_or_else(|| {
                    StoreError::InvalidEvent("pending steer requires a root input".into())
                })?;
                let target_input = self.agent_inputs.get(target).ok_or_else(|| {
                    StoreError::InvalidEvent(format!("unknown steer root input {target}"))
                })?;
                if target_input.input.agent_instance_id != input.agent_instance_id
                    || target_input.disposition != AgentInputDisposition::AppliedAsRoot
                    || self.active_root_by_agent.get(&input.agent_instance_id)
                        != Some(&target.to_string())
                {
                    return Err(StoreError::InvalidEvent(
                        "steer root is not an active root input".into(),
                    ));
                }
                Some(target.to_string())
            }
            AgentInputDisposition::PendingFollowUp => {
                if data.root_input_id.is_some() {
                    return Err(StoreError::InvalidEvent(
                        "follow-up admission cannot have a root input".into(),
                    ));
                }
                None
            }
            AgentInputDisposition::Cancelled => {
                return Err(StoreError::InvalidEvent(
                    "an admitted input cannot start cancelled".into(),
                ));
            }
            AgentInputDisposition::AppliedToStep => {
                return Err(StoreError::InvalidEvent(
                    "an admitted input cannot start applied_to_step".into(),
                ));
            }
        };
        if let Some(existing) = self.agent_inputs.get(&input.input_id) {
            return if existing.input == input
                && existing.admission_disposition == disposition
                && existing.admission_root_input_id == root_input_id
            {
                Ok(())
            } else {
                Err(StoreError::IdempotencyConflict(input.input_id))
            };
        }
        if self
            .agent_inputs
            .values()
            .any(|existing| existing.input.request_id == input.request_id)
        {
            return Err(StoreError::IdempotencyConflict(input.request_id));
        }
        if disposition == AgentInputDisposition::AppliedAsRoot
            && self.agent_inputs.values().any(|existing| {
                existing.input.agent_instance_id == input.agent_instance_id
                    && existing.input.input_id != input.input_id
                    && existing.has_unfinished_processing()
            })
        {
            return Err(StoreError::InvalidEvent(
                "agent already has an active root work".into(),
            ));
        }
        let input_id = input.input_id.clone();
        self.agent_inputs.insert(
            input_id,
            StoredAgentInput {
                input,
                admission_disposition: disposition,
                admission_root_input_id: root_input_id.clone(),
                disposition,
                admission_revision: revision,
                admission_event_id: raw.event_id.clone(),
                admitted_at: data.admitted_at,
                root_input_id,
                model_step_id: None,
                applied_message_id: None,
                processing: None,
            },
        );
        Ok(())
    }

    pub(crate) fn apply_agent_input_disposition_changed(
        &mut self,
        data: AgentInputDispositionChangedV1,
    ) -> Result<()> {
        let disposition = canonical_disposition(data.disposition)?;
        let input = self
            .agent_inputs
            .get(&data.input_id)
            .ok_or_else(|| StoreError::InvalidEvent(format!("unknown input {}", data.input_id)))?;
        if input.input.agent_instance_id != data.agent_instance_id {
            return Err(StoreError::InvalidEvent(
                "input transition belongs to another agent".into(),
            ));
        }
        let root_input_id = if disposition == AgentInputDisposition::AppliedAsRoot {
            Some(data.input_id.clone())
        } else {
            data.root_input_id
                .clone()
                .or_else(|| input.root_input_id.clone())
        };
        let model_step_id = data
            .model_step_id
            .clone()
            .or_else(|| input.model_step_id.clone());
        if input.disposition == disposition
            && input.root_input_id == root_input_id
            && input.model_step_id == model_step_id
        {
            return Ok(());
        }
        if !valid_disposition_transition(input.disposition, disposition) {
            return Err(StoreError::InvalidEvent(format!(
                "invalid input disposition transition {:?} -> {:?}",
                input.disposition, disposition
            )));
        }
        if disposition == AgentInputDisposition::AppliedAsRoot
            && root_input_id.as_deref() != Some(data.input_id.as_str())
        {
            return Err(StoreError::InvalidEvent(
                "root application must identify the input itself".into(),
            ));
        }
        if disposition == AgentInputDisposition::AppliedAsRoot
            && self.agent_inputs.values().any(|existing| {
                existing.input.agent_instance_id == data.agent_instance_id
                    && existing.input.input_id != data.input_id
                    && existing.has_unfinished_processing()
            })
        {
            return Err(StoreError::InvalidEvent(
                "agent already has an active root work".into(),
            ));
        }
        if disposition == AgentInputDisposition::AppliedToStep {
            let root_input_id = root_input_id.as_deref().ok_or_else(|| {
                StoreError::InvalidEvent("step application requires a root input".into())
            })?;
            let root = self.agent_inputs.get(root_input_id).ok_or_else(|| {
                StoreError::InvalidEvent(format!("unknown application root input {root_input_id}"))
            })?;
            if root.input.agent_instance_id != data.agent_instance_id
                || root.disposition != AgentInputDisposition::AppliedAsRoot
            {
                return Err(StoreError::InvalidEvent(
                    "step application root is invalid".into(),
                ));
            }
            if self.active_root_by_agent.get(&data.agent_instance_id)
                != Some(&root_input_id.to_string())
            {
                return Err(StoreError::InvalidEvent(
                    "step application requires the active root input".into(),
                ));
            }
            if model_step_id.as_deref().is_none_or(str::is_empty) {
                return Err(StoreError::InvalidEvent(
                    "step application requires a model step".into(),
                ));
            }
            if self
                .model_steps
                .contains_key(model_step_id.as_deref().unwrap_or_default())
            {
                return Err(StoreError::InvalidEvent(
                    "step application must precede model step commit".into(),
                ));
            }
        }
        if disposition == AgentInputDisposition::Cancelled {
            match input.disposition {
                AgentInputDisposition::PendingFollowUp
                    if root_input_id.is_some() || model_step_id.is_some() =>
                {
                    return Err(StoreError::InvalidEvent(
                        "follow-up cancellation cannot add causal bindings".into(),
                    ));
                }
                AgentInputDisposition::PendingSteer
                    if root_input_id != input.root_input_id || model_step_id.is_some() =>
                {
                    return Err(StoreError::InvalidEvent(
                        "steer cancellation cannot change causal bindings".into(),
                    ));
                }
                _ => {}
            }
        }
        let input = self
            .agent_inputs
            .get_mut(&data.input_id)
            .expect("input validated above");
        input.disposition = disposition;
        input.root_input_id = root_input_id;
        input.model_step_id = model_step_id;
        Ok(())
    }

    pub(crate) fn apply_agent_input_processing_started(
        &mut self,
        data: AgentInputProcessingStartedV1,
    ) -> Result<()> {
        let AgentInputProcessingStartedV1 {
            agent_instance_id,
            root_input_id,
            request_id,
            base_message_id,
            tree_base_entry_id,
            detached_recipient_agent_instance_id,
            prompt_assembly_version,
            prompt_digest,
            started_at,
        } = data;
        if root_input_id.is_empty() {
            return Err(StoreError::InvalidEvent(
                "processing start requires a root input".into(),
            ));
        }
        let input = self.agent_inputs.get(&root_input_id).ok_or_else(|| {
            StoreError::InvalidEvent(format!("unknown root input {root_input_id}"))
        })?;
        if input.input.agent_instance_id != agent_instance_id
            || input.input.request_id != request_id
            || input.disposition != AgentInputDisposition::AppliedAsRoot
            || input.root_input_id.as_deref() != Some(root_input_id.as_str())
        {
            return Err(StoreError::InvalidEvent(
                "processing start does not target an applied root input".into(),
            ));
        }
        let processing = StoredRootProcessing {
            started_at,
            finished_at: None,
            report: None,
            base_message_id,
            tree_base_entry_id,
            root_input_id: Some(root_input_id.clone()),
            detached_recipient_agent_instance_id,
            prompt_assembly_version,
            prompt_digest,
        };
        if let Some(existing) = &input.processing {
            return if existing == &processing {
                Ok(())
            } else {
                Err(StoreError::IdempotencyConflict(root_input_id))
            };
        }
        if self.agent_inputs.values().any(|existing| {
            existing.input.agent_instance_id == agent_instance_id
                && existing.input.input_id != root_input_id
                && existing.has_unfinished_processing()
        }) {
            return Err(StoreError::InvalidEvent(
                "agent already has an active root work".into(),
            ));
        }
        self.agent_inputs
            .get_mut(&root_input_id)
            .expect("root input validated above")
            .processing = Some(processing);
        Ok(())
    }

    pub(crate) fn apply_agent_input_processing_finished(
        &mut self,
        data: AgentInputProcessingFinishedV1,
    ) -> Result<()> {
        let AgentInputProcessingFinishedV1 {
            agent_instance_id,
            root_input_id,
            report,
            finished_at,
        } = data;
        let input = self.agent_inputs.get(&root_input_id).ok_or_else(|| {
            StoreError::InvalidEvent(format!("unknown root input {root_input_id}"))
        })?;
        if input.input.agent_instance_id != agent_instance_id {
            return Err(StoreError::InvalidEvent(
                "processing finish belongs to another agent".into(),
            ));
        }
        let Some(processing) = &input.processing else {
            return Err(StoreError::InvalidEvent(
                "processing finish without a processing start".into(),
            ));
        };
        if processing.finished_at.is_some() {
            return if processing.finished_at == Some(finished_at)
                && processing.report.as_ref() == Some(&report)
            {
                Ok(())
            } else {
                Err(StoreError::IdempotencyConflict(root_input_id))
            };
        }
        if report.agent_instance_id != agent_instance_id || report.root_input_id != root_input_id {
            return Err(StoreError::InvalidEvent(
                "invalid processing completion".into(),
            ));
        }
        let input = self
            .agent_inputs
            .get_mut(&root_input_id)
            .expect("root input validated above");
        let processing = input
            .processing
            .as_mut()
            .expect("processing validated above");
        processing.report = Some(report);
        processing.finished_at = Some(finished_at);
        Ok(())
    }

    /// Rebuild canonical input indexes and all derived Agent work snapshots
    /// before a query consumes an aggregate loaded from an older read model.
    pub fn rebuild_work_projection(&mut self) {
        self.rebuild_agent_input_indexes();
        self.agent_work = self.agent_work_snapshots();
    }

    /// The applied root input whose processing is unfinished, if any.
    pub(crate) fn unfinished_root(
        &self,
        agent_instance_id: &str,
    ) -> Option<(&StoredAgentInput, &StoredRootProcessing)> {
        self.agent_inputs.values().find_map(|input| {
            let processing = input.processing.as_ref()?;
            (input.input.agent_instance_id == agent_instance_id && processing.finished_at.is_none())
                .then_some((input, processing))
        })
    }

    /// The root input and its processing facts.
    pub fn work_by_root_input_id(
        &self,
        root_input_id: &str,
    ) -> Option<(&StoredAgentInput, &StoredRootProcessing)> {
        let input = self.agent_inputs.get(root_input_id)?;
        let processing = input.processing.as_ref()?;
        Some((input, processing))
    }

    /// Last message committed under this root input's work.
    pub fn work_message_head(&self, root_input_id: &str) -> Option<&str> {
        self.messages
            .values()
            .filter(|message| message.data.root_input_id.as_deref() == Some(root_input_id))
            .max_by_key(|message| message.revision)
            .map(|message| message.data.message_id.as_str())
    }

    /// Number of model steps committed under this root input's work.
    pub fn work_model_step_count(&self, root_input_id: &str) -> usize {
        self.model_steps
            .values()
            .filter(|step| step.data.root_input_id == root_input_id)
            .count()
    }
}

pub(crate) fn canonical_disposition(
    disposition: AgentInputDisposition,
) -> Result<AgentInputDisposition> {
    Ok(disposition)
}

fn valid_disposition_transition(from: AgentInputDisposition, to: AgentInputDisposition) -> bool {
    matches!(
        (from, to),
        (
            AgentInputDisposition::PendingFollowUp,
            AgentInputDisposition::AppliedAsRoot | AgentInputDisposition::Cancelled
        ) | (
            AgentInputDisposition::PendingSteer,
            AgentInputDisposition::AppliedToStep | AgentInputDisposition::Cancelled
        )
    )
}
