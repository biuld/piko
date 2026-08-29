//! Compatibility upcasts and indexes for the canonical AgentInput view.

use piko_protocol::{
    AgentInput, AgentInputDisposition, AgentInputOrigin, DurableAgentInput, Message,
};

use crate::aggregate::SessionAggregate;
use crate::projection::StoredAgentInput;
use crate::{Result, StoreError};

impl SessionAggregate {
    pub(crate) fn apply_input_queued(&mut self, input: DurableAgentInput) -> Result<()> {
        self.ensure_agent(&input.request.agent_instance_id)?;
        if Some(input.request.session_id.as_str()) != self.session_id.as_deref() {
            return Err(StoreError::InvalidEvent(
                "input belongs to another session".into(),
            ));
        }
        if let Some(existing) = self.agent_inputs.get(&input.queued_input_id) {
            if same_legacy_input(existing, &input) {
                if existing.disposition != AgentInputDisposition::PendingFollowUp {
                    // Canonical disposition facts are authoritative. A late
                    // compatibility queue event must not resurrect an input
                    // that was already applied or cancelled.
                    return Ok(());
                }
                if let Some(queued) = self
                    .queued_inputs
                    .iter_mut()
                    .find(|queued| queued.queued_input_id == input.queued_input_id)
                {
                    // The canonical AgentInput intentionally omits the legacy
                    // message_id field. Keep the compatibility queue payload
                    // from the old event when the dual-write follows it.
                    *queued = input;
                } else {
                    self.queued_inputs.push(input);
                }
                return Ok(());
            }
            return Err(StoreError::InvalidEvent("duplicate queued input".into()));
        }
        if self
            .queued_inputs
            .iter()
            .any(|existing| existing.queued_input_id == input.queued_input_id)
        {
            return Err(StoreError::InvalidEvent("duplicate queued input".into()));
        }
        let stored_input = legacy_agent_input(&input);
        self.agent_inputs.insert(
            input.queued_input_id.clone(),
            StoredAgentInput {
                input: stored_input,
                admission_disposition: AgentInputDisposition::PendingFollowUp,
                admission_root_input_id: None,
                admission_run_id: None,
                admission_bound_run_id: None,
                disposition: AgentInputDisposition::PendingFollowUp,
                admission_revision: self.revision + 1,
                admission_event_id: format!("legacy:input:{}", input.queued_input_id),
                admitted_at: 0,
                root_input_id: None,
                run_id: None,
                bound_run_id: None,
                model_step_id: None,
            },
        );
        self.queued_inputs.push(input);
        Ok(())
    }

    pub(crate) fn rebuild_agent_input_indexes(&mut self) {
        self.upcast_legacy_root_inputs();
        self.rebuild_compatibility_queue();
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
            if let Some(input_id) = self.input_by_request.get(&execution.started.request_id)
                && self.agent_inputs.contains_key(input_id)
            {
                self.active_root_by_agent.insert(
                    execution.started.agent_instance_id.clone(),
                    input_id.clone(),
                );
            }
        }
        self.agent_work = self.agent_work_snapshots();
    }

    fn rebuild_compatibility_queue(&mut self) {
        let old_queue = std::mem::take(&mut self.queued_inputs);
        let mut queued_ids = std::collections::BTreeSet::new();
        let mut queue = Vec::with_capacity(old_queue.len());
        for input in old_queue {
            match self.agent_inputs.get(&input.queued_input_id) {
                Some(stored) if stored.disposition != AgentInputDisposition::PendingFollowUp => {
                    continue;
                }
                Some(_) => {
                    queued_ids.insert(input.queued_input_id.clone());
                }
                None => {}
            }
            queue.push(input);
        }
        let mut pending = self
            .agent_inputs
            .values()
            .filter(|input| {
                input.disposition == AgentInputDisposition::PendingFollowUp
                    && !queued_ids.contains(&input.input.input_id)
            })
            .collect::<Vec<_>>();
        pending.sort_by_key(|input| (input.admission_revision, input.input.input_id.clone()));
        queue.extend(pending.into_iter().map(|input| DurableAgentInput {
            queued_input_id: input.input.input_id.clone(),
            request: input.input.to_request(),
            submitted_at: Some(input.input.submitted_at),
            detached_recipient_agent_instance_id: None,
        }));
        self.queued_inputs = queue;
    }

    fn upcast_legacy_root_inputs(&mut self) {
        let candidates = self
            .executions
            .values()
            .filter_map(|execution| {
                let first_user = self
                    .messages
                    .values()
                    .filter(|message| {
                        message.data.execution_id.as_deref()
                            == Some(execution.started.execution_id.as_str())
                            && matches!(&message.data.message, Message::User { .. })
                    })
                    .min_by_key(|message| message.revision)?;
                Some((
                    execution.started.clone(),
                    first_user.revision,
                    first_user.data.clone(),
                ))
            })
            .collect::<Vec<_>>();

        for (started, message_revision, message) in candidates {
            let Message::User { content, .. } = message.message else {
                continue;
            };
            let input_id = started.request_id.clone();
            if self
                .agent_inputs
                .values()
                .any(|input| input.input.request_id == input_id)
                || self.agent_inputs.contains_key(&input_id)
            {
                continue;
            }
            let input = AgentInput {
                input_id: input_id.clone(),
                request_id: input_id.clone(),
                session_id: self
                    .session_id
                    .clone()
                    .unwrap_or_else(|| started.agent_instance_id.clone()),
                agent_instance_id: started.agent_instance_id.clone(),
                origin: if started.source_turn_id.is_some() {
                    AgentInputOrigin::User
                } else {
                    AgentInputOrigin::System
                },
                delivery: piko_protocol::AgentInputDelivery::StartWhenIdle,
                content,
                submitted_at: message.committed_at,
                user_turn_id: started.source_turn_id.clone(),
                caller_agent_instance_id: None,
            };
            self.agent_inputs.insert(
                input_id.clone(),
                StoredAgentInput {
                    input,
                    admission_disposition: AgentInputDisposition::AppliedAsRoot,
                    admission_root_input_id: Some(input_id.clone()),
                    admission_run_id: Some(started.run_id.clone()),
                    admission_bound_run_id: None,
                    disposition: AgentInputDisposition::AppliedAsRoot,
                    admission_revision: message_revision,
                    admission_event_id: format!("legacy:root:{input_id}"),
                    admitted_at: message.committed_at,
                    root_input_id: Some(input_id),
                    run_id: Some(started.run_id),
                    bound_run_id: None,
                    model_step_id: None,
                },
            );
        }
    }

    pub(crate) fn ensure_agent(&self, agent_instance_id: &str) -> Result<()> {
        self.agents
            .get(agent_instance_id)
            .map(|_| ())
            .ok_or_else(|| StoreError::InvalidEvent(format!("unknown agent {agent_instance_id}")))
    }
}

fn same_legacy_input(stored: &StoredAgentInput, queued: &DurableAgentInput) -> bool {
    let legacy = legacy_agent_input(queued);
    stored.input.input_id == legacy.input_id
        && stored.input.request_id == legacy.request_id
        && stored.input.session_id == legacy.session_id
        && stored.input.agent_instance_id == legacy.agent_instance_id
        && stored.input.origin == legacy.origin
        && stored.input.delivery == legacy.delivery
        && stored.input.content == legacy.content
        && stored.input.user_turn_id == legacy.user_turn_id
        && stored.input.caller_agent_instance_id == legacy.caller_agent_instance_id
}

pub(crate) fn legacy_agent_input(queued: &DurableAgentInput) -> AgentInput {
    let request = &queued.request;
    AgentInput {
        input_id: queued.queued_input_id.clone(),
        request_id: request.request_id.clone(),
        session_id: request.session_id.clone(),
        agent_instance_id: request.agent_instance_id.clone(),
        origin: if request.caller_agent_instance_id.is_some() {
            AgentInputOrigin::Agent
        } else if request.source_turn_id.is_some() {
            AgentInputOrigin::User
        } else {
            AgentInputOrigin::System
        },
        delivery: request.delivery,
        content: request.content.clone(),
        submitted_at: queued.submitted_at.unwrap_or(0),
        user_turn_id: request.source_turn_id.clone(),
        caller_agent_instance_id: request.caller_agent_instance_id.clone(),
    }
}
