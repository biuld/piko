//! Deterministic Agent work read-model projection.

use std::collections::BTreeMap;

use piko_protocol::{
    ActiveWorkSnapshot, AgentInputDisposition, AgentInputSummary, AgentWorkSnapshot,
    AgentWorkViewState,
};

use crate::aggregate::SessionAggregate;
use crate::projection::StoredAgentInput;

impl SessionAggregate {
    /// Deterministic per-agent work projection used by current read models.
    pub fn agent_work_snapshots(&self) -> BTreeMap<String, AgentWorkSnapshot> {
        self.agents
            .keys()
            .map(|agent_instance_id| {
                (
                    agent_instance_id.clone(),
                    self.agent_work_snapshot(agent_instance_id),
                )
            })
            .collect()
    }

    pub fn agent_work_snapshot(&self, agent_instance_id: &str) -> AgentWorkSnapshot {
        let stored_inputs: Vec<&StoredAgentInput> = self
            .agent_inputs
            .values()
            .filter(|input| input.input.agent_instance_id == agent_instance_id)
            .collect();
        let active_execution = self
            .executions
            .values()
            .filter(|execution| {
                execution.started.agent_instance_id == agent_instance_id
                    && execution.finished_at.is_none()
            })
            .min_by_key(|execution| execution.started.started_at);
        let active_input = active_execution.and_then(|execution| {
            stored_inputs
                .iter()
                .find(|input| input.input.request_id == execution.started.request_id)
                .copied()
        });
        let active_work = active_execution.map(|execution| {
            let root_input_id = active_input
                .and_then(|input| input.root_input_id.clone())
                .unwrap_or_else(|| execution.started.request_id.clone());
            let active_model_step_id = stored_inputs
                .iter()
                .filter(|input| {
                    input.disposition == AgentInputDisposition::AppliedToStep
                        && input.root_input_id.as_deref() == Some(root_input_id.as_str())
                })
                .filter_map(|input| {
                    let step_id = input.model_step_id.as_ref()?;
                    (!self.model_steps.contains_key(step_id))
                        .then_some((input.admission_revision, step_id.clone()))
                })
                .max_by_key(|(revision, _)| *revision)
                .map(|(_, step_id)| step_id);
            ActiveWorkSnapshot {
                root_input_id,
                state: AgentWorkViewState::Running,
                active_model_step_id,
                started_at: execution.started.started_at,
            }
        });
        let mut pending_steers = Vec::new();
        let mut queued_inputs = Vec::new();
        for input in stored_inputs {
            let summary = input_summary(input);
            match input.disposition {
                AgentInputDisposition::PendingSteer => pending_steers.push(summary),
                AgentInputDisposition::PendingFollowUp => queued_inputs.push(summary),
                _ => {}
            }
        }
        pending_steers.sort_by_key(|input| (input.admission_revision, input.input_id.clone()));
        queued_inputs.sort_by_key(|input| (input.admission_revision, input.input_id.clone()));
        let pending_action = None;
        let foreground = piko_protocol::AgentForeground::project_work(
            active_work.as_ref(),
            pending_action.as_ref(),
            !queued_inputs.is_empty(),
        );
        let lifecycle = self.agents.get(agent_instance_id).map_or(
            piko_protocol::AgentInstanceLifecycle::Unavailable,
            |agent| agent.lifecycle,
        );
        AgentWorkSnapshot {
            agent_instance_id: agent_instance_id.to_string(),
            lifecycle,
            foreground,
            active_work,
            pending_steers,
            queued_inputs,
            pending_action,
        }
    }
}

fn input_summary(input: &StoredAgentInput) -> AgentInputSummary {
    AgentInputSummary {
        input_id: input.input.input_id.clone(),
        origin: input.input.origin,
        preview: input.input.preview(),
        admission_revision: input.admission_revision,
        submitted_at: input.input.submitted_at,
        delivery: input.input.delivery,
        disposition: input.disposition,
    }
}
