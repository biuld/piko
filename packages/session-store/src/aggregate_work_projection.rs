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
        let active_work_input = self.unfinished_root(agent_instance_id);
        let active_work = active_work_input.map(|(input, processing)| {
            let root_input_id = input.input.input_id.clone();
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
                root_input_id: root_input_id.clone(),
                state: unfinished_work_state(self, &root_input_id),
                active_model_step_id,
                started_at: processing.started_at,
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
        let pending_action = active_work.as_ref().and_then(|active| {
            self.pending_actions
                .values()
                .filter(|pending| {
                    pending.agent_instance_id == agent_instance_id
                        && pending.root_input_id == active.root_input_id
                })
                .max_by_key(|pending| pending.requested_at)
                .map(|pending| pending.action.clone())
        });
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

fn unfinished_work_state(aggregate: &SessionAggregate, root_input_id: &str) -> AgentWorkViewState {
    if aggregate.interrupt_requested_roots.contains(root_input_id) {
        return AgentWorkViewState::Cancelling;
    }
    if aggregate
        .pending_actions
        .values()
        .any(|pending| pending.root_input_id == root_input_id)
    {
        return AgentWorkViewState::RequiresAction;
    }
    // The initiating user message is part of the processing-start semantic
    // commit, so its presence no longer distinguishes Starting from Running.
    // A committed model boundary is the first durable evidence that model
    // processing advanced beyond startup; finer live progress is overlaid by
    // hostd without inventing another durable status fact.
    if aggregate.work_model_step_count(root_input_id) > 0 {
        AgentWorkViewState::Running
    } else {
        AgentWorkViewState::Starting
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
