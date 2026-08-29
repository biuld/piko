//! Deterministic Agent work read-model projection.

use std::collections::BTreeMap;

use piko_protocol::{
    ActiveRunSnapshot, AgentInputDisposition, AgentInputSummary, AgentRunViewState,
    AgentWorkSnapshot, DurableAgentInput,
};

use crate::aggregate::SessionAggregate;
use crate::aggregate_work_compat::legacy_agent_input;
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
        let active_run = active_execution.map(|execution| ActiveRunSnapshot {
            run_id: execution.started.run_id.clone(),
            root_input_id: active_input
                .and_then(|input| input.root_input_id.clone())
                .unwrap_or_else(|| execution.started.request_id.clone()),
            user_turn_id: active_input
                .and_then(|input| input.input.user_turn_id.clone())
                .or_else(|| execution.started.source_turn_id.clone()),
            state: AgentRunViewState::Running,
            active_model_step_id: None,
            started_at: execution.started.started_at,
        });
        let canonical_input_ids = stored_inputs
            .iter()
            .map(|input| input.input.input_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
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
        for queued in self.queued_inputs.iter().filter(|queued| {
            queued.request.agent_instance_id == agent_instance_id
                && !canonical_input_ids.contains(queued.queued_input_id.as_str())
        }) {
            queued_inputs.push(legacy_input_summary(queued));
        }
        pending_steers.sort_by_key(|input| (input.admission_revision, input.input_id.clone()));
        queued_inputs.sort_by_key(|input| (input.admission_revision, input.input_id.clone()));
        let pending_action = None;
        let foreground = piko_protocol::AgentForeground::project_work(
            active_run.as_ref(),
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
            active_run,
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
        user_turn_id: input.input.user_turn_id.clone(),
        disposition: input.disposition,
    }
}

fn legacy_input_summary(input: &DurableAgentInput) -> AgentInputSummary {
    let input = legacy_agent_input(input);
    let preview = input.preview();
    AgentInputSummary {
        input_id: input.input_id,
        origin: input.origin,
        preview,
        admission_revision: 0,
        submitted_at: input.submitted_at,
        delivery: input.delivery,
        user_turn_id: input.user_turn_id,
        disposition: AgentInputDisposition::PendingFollowUp,
    }
}
