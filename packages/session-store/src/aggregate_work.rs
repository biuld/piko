//! AgentInput reducers and the derived work indexes/snapshot.

use piko_protocol::{AgentInputDisposition, DurableAgentInput};

use crate::aggregate::SessionAggregate;
use crate::schema::{
    AgentInputAdmittedV1, AgentInputDispositionChangedV1, ExecutionStartedV1, RawEvent,
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
                {
                    return Err(StoreError::InvalidEvent(
                        "steer root is not an active root input".into(),
                    ));
                }
                let bound_run_id = data.bound_run_id.as_deref().ok_or_else(|| {
                    StoreError::InvalidEvent("pending steer requires a bound run".into())
                })?;
                if target_input.run_id.as_deref() != Some(bound_run_id)
                    || !self.executions.values().any(|execution| {
                        execution.started.agent_instance_id == input.agent_instance_id
                            && execution.started.run_id == bound_run_id
                            && execution.finished_at.is_none()
                    })
                {
                    return Err(StoreError::InvalidEvent(
                        "pending steer is not bound to the active root run".into(),
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
            _ => {
                return Err(StoreError::InvalidEvent(
                    "unsupported admitted input disposition".into(),
                ));
            }
        };
        if let Some(existing) = self.agent_inputs.get(&input.input_id) {
            return if existing.input == input
                && existing.admission_disposition == disposition
                && existing.admission_root_input_id == root_input_id
                && existing.admission_run_id == data.run_id
                && existing.admission_bound_run_id == data.bound_run_id
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
            && self.executions.values().any(|execution| {
                execution.started.agent_instance_id == input.agent_instance_id
                    && execution.finished_at.is_none()
                    && (execution.started.request_id != input.request_id
                        || data.run_id.as_deref() != Some(execution.started.run_id.as_str()))
            })
        {
            return Err(StoreError::InvalidEvent(
                "agent already has an active root run".into(),
            ));
        }
        let input_id = input.input_id.clone();
        let queued = disposition == AgentInputDisposition::PendingFollowUp;
        let request = input.to_request();
        self.agent_inputs.insert(
            input_id.clone(),
            StoredAgentInput {
                input,
                admission_disposition: disposition,
                admission_root_input_id: root_input_id.clone(),
                admission_run_id: data.run_id.clone(),
                admission_bound_run_id: data.bound_run_id.clone(),
                disposition,
                admission_revision: revision,
                admission_event_id: raw.event_id.clone(),
                admitted_at: data.admitted_at,
                root_input_id,
                run_id: data.run_id,
                bound_run_id: data.bound_run_id,
                model_step_id: None,
            },
        );
        if queued
            && !self
                .queued_inputs
                .iter()
                .any(|queued| queued.queued_input_id == input_id)
        {
            self.queued_inputs.push(DurableAgentInput {
                queued_input_id: input_id.clone(),
                request,
                submitted_at: self
                    .agent_inputs
                    .get(&input_id)
                    .map(|input| input.input.submitted_at),
                detached_recipient_agent_instance_id: None,
            });
        }
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
        let run_id = data.run_id.clone().or_else(|| input.run_id.clone());
        let bound_run_id = data
            .bound_run_id
            .clone()
            .or_else(|| input.bound_run_id.clone());
        let model_step_id = data
            .model_step_id
            .clone()
            .or_else(|| input.model_step_id.clone());
        if input.disposition == disposition
            && input.root_input_id == root_input_id
            && input.run_id == run_id
            && input.bound_run_id == bound_run_id
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
            && run_id.as_deref().is_none_or(str::is_empty)
        {
            return Err(StoreError::InvalidEvent(
                "root application requires a run".into(),
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
            let run_id = run_id.as_deref().ok_or_else(|| {
                StoreError::InvalidEvent("step application requires a run".into())
            })?;
            let bound_run_id = bound_run_id.as_deref().ok_or_else(|| {
                StoreError::InvalidEvent("step application requires a bound run".into())
            })?;
            if root.run_id.as_deref() != Some(run_id) || bound_run_id != run_id {
                return Err(StoreError::InvalidEvent(
                    "step application run does not match its root".into(),
                ));
            }
            if !self.executions.values().any(|execution| {
                execution.started.agent_instance_id == data.agent_instance_id
                    && execution.started.run_id == run_id
                    && execution.finished_at.is_none()
            }) {
                return Err(StoreError::InvalidEvent(
                    "step application requires an active run".into(),
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
                    if root_input_id.is_some()
                        || run_id.is_some()
                        || bound_run_id.is_some()
                        || model_step_id.is_some() =>
                {
                    return Err(StoreError::InvalidEvent(
                        "follow-up cancellation cannot add causal bindings".into(),
                    ));
                }
                AgentInputDisposition::PendingSteer
                    if root_input_id != input.root_input_id
                        || run_id != input.run_id
                        || bound_run_id != input.bound_run_id
                        || model_step_id.is_some() =>
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
        input.run_id = run_id;
        input.bound_run_id = bound_run_id;
        input.model_step_id = model_step_id;
        if disposition != AgentInputDisposition::PendingFollowUp {
            self.queued_inputs
                .retain(|queued| queued.queued_input_id != data.input_id);
        }
        Ok(())
    }

    pub(crate) fn apply_execution_started_with_input(
        &mut self,
        started: &ExecutionStartedV1,
    ) -> Result<()> {
        let Some(input) = self
            .agent_inputs
            .values_mut()
            .find(|input| input.input.request_id == started.request_id)
        else {
            return Ok(());
        };
        if input.input.agent_instance_id != started.agent_instance_id {
            return Err(StoreError::InvalidEvent(
                "execution root input belongs to another agent".into(),
            ));
        }
        if input.disposition != AgentInputDisposition::AppliedAsRoot
            || input.root_input_id.as_deref() != Some(input.input.input_id.as_str())
        {
            return Err(StoreError::InvalidEvent(
                "execution request does not resolve to an applied root input".into(),
            ));
        }
        if let Some(existing_run_id) = &input.run_id {
            if existing_run_id != &started.run_id {
                return Err(StoreError::InvalidEvent(
                    "execution root input is bound to another run".into(),
                ));
            }
        } else {
            input.run_id = Some(started.run_id.clone());
        }
        Ok(())
    }

    /// Rebuild canonical input indexes and all derived Agent work snapshots
    /// before a query consumes an aggregate loaded from an older read model.
    pub fn rebuild_work_projection(&mut self) {
        self.rebuild_agent_input_indexes();
    }
}

pub(crate) fn canonical_disposition(
    disposition: AgentInputDisposition,
) -> Result<AgentInputDisposition> {
    match disposition {
        AgentInputDisposition::Accepted => Ok(AgentInputDisposition::AppliedAsRoot),
        AgentInputDisposition::Queued => Ok(AgentInputDisposition::PendingFollowUp),
        AgentInputDisposition::PendingFollowUp
        | AgentInputDisposition::PendingSteer
        | AgentInputDisposition::AppliedAsRoot
        | AgentInputDisposition::AppliedToStep
        | AgentInputDisposition::Cancelled => Ok(disposition),
        AgentInputDisposition::Duplicate | AgentInputDisposition::Overload => Err(
            StoreError::InvalidEvent("duplicate or overload is not a durable disposition".into()),
        ),
    }
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
