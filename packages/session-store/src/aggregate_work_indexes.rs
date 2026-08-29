//! Derived indexes reconstructed from AgentInput facts.

use piko_protocol::{AgentInput, AgentInputDisposition};

use crate::aggregate::SessionAggregate;
use crate::{Result, StoreError};

impl SessionAggregate {
    pub(crate) fn rebuild_agent_input_indexes(&mut self) {
        self.active_root_by_agent.clear();
        self.pending_inputs_by_agent.clear();
        self.input_by_request.clear();
        for (input_id, stored) in &self.agent_inputs {
            self.input_by_request
                .insert(stored.input.request_id.clone(), input_id.clone());
            if matches!(
                stored.disposition,
                AgentInputDisposition::PendingFollowUp | AgentInputDisposition::PendingSteer
            ) {
                self.pending_inputs_by_agent
                    .entry(stored.input.agent_instance_id.clone())
                    .or_default()
                    .push(input_id.clone());
            }
        }
        for input_ids in self.pending_inputs_by_agent.values_mut() {
            input_ids.sort_by_key(|input_id| {
                self.agent_inputs
                    .get(input_id)
                    .map(|input| (input.admission_revision, input.input.input_id.clone()))
            });
        }
        for execution in self
            .executions
            .values()
            .filter(|execution| execution.finished_at.is_none())
        {
            if let Some(input_id) = self.input_by_request.get(&execution.started.request_id) {
                self.active_root_by_agent.insert(
                    execution.started.agent_instance_id.clone(),
                    input_id.clone(),
                );
            }
        }
        self.agent_work = self.agent_work_snapshots();
    }

    pub fn pending_follow_ups(&self, agent_instance_id: Option<&str>) -> Vec<AgentInput> {
        let mut pending = self
            .agent_inputs
            .values()
            .filter(|input| input.disposition == AgentInputDisposition::PendingFollowUp)
            .filter(|input| agent_instance_id.is_none_or(|id| input.input.agent_instance_id == id))
            .collect::<Vec<_>>();
        pending.sort_by_key(|input| (input.admission_revision, input.input.input_id.clone()));
        pending
            .into_iter()
            .map(|input| input.input.clone())
            .collect()
    }

    pub(crate) fn ensure_agent(&self, agent_instance_id: &str) -> Result<()> {
        self.agents
            .get(agent_instance_id)
            .map(|_| ())
            .ok_or_else(|| StoreError::InvalidEvent(format!("unknown agent {agent_instance_id}")))
    }
}
