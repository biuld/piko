use async_trait::async_trait;
use piko_orchd_api::AgentCommitPort;
use piko_protocol::{AgentCommitAck, AgentDurableCommand, AgentInboxItem, CommitError};
use piko_session_store::{EventData, ExecutionStartedV1};

use super::SessionStore;

#[async_trait]
impl AgentCommitPort for SessionStore {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let session_id = session_id.to_string();
        self.run_durable(move |store| store.commit_agent_command_unlocked(&session_id, command))
            .await
    }
}

impl SessionStore {
    fn commit_agent_command_unlocked(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let aggregate = self.aggregate().map_err(Self::commit_error)?;
        if aggregate.session_id.as_deref() != Some(session_id) {
            return Err(CommitError::IdentityMismatch);
        }
        let (commit_id, agent_instance_id, committed_at, events) = match command {
            AgentDurableCommand::Create { identity, spec } => {
                if identity.session_id != session_id {
                    return Err(CommitError::IdentityMismatch);
                }
                if let Some(existing) = aggregate.agents.get(&identity.agent_instance_id) {
                    if existing.identity != identity {
                        return Err(CommitError::IdempotencyConflict);
                    }
                    if let Some(existing_spec) = &existing.spec {
                        if existing_spec != &spec {
                            return Err(CommitError::IdempotencyConflict);
                        }
                        return Ok(agent_ack(
                            session_id,
                            &identity.agent_instance_id,
                            aggregate.revision,
                        ));
                    }
                }
                let now = chrono::Utc::now().timestamp_millis();
                (
                    stable("agent-create", &[session_id, &identity.agent_instance_id]),
                    identity.agent_instance_id.clone(),
                    now,
                    vec![EventData::AgentCreated {
                        identity,
                        spec,
                        created_at: now,
                    }],
                )
            }
            AgentDurableCommand::SetLifecycle {
                agent_instance_id,
                lifecycle,
            } => {
                let agent = aggregate
                    .agents
                    .get(&agent_instance_id)
                    .ok_or(CommitError::IdentityMismatch)?;
                if agent.lifecycle == lifecycle {
                    return Ok(agent_ack(
                        session_id,
                        &agent_instance_id,
                        aggregate.revision,
                    ));
                }
                let now = chrono::Utc::now().timestamp_millis();
                (
                    stable(
                        "agent-lifecycle",
                        &[session_id, &agent_instance_id, &format!("{lifecycle:?}")],
                    ),
                    agent_instance_id.clone(),
                    now,
                    vec![EventData::AgentLifecycleChanged {
                        agent_instance_id,
                        lifecycle,
                        changed_at: now,
                    }],
                )
            }
            AgentDurableCommand::RunStarted {
                agent_instance_id,
                run_id,
                internal_execution_id,
                request_id,
                source_turn_id,
                detached_recipient_agent_instance_id,
                prompt_assembly_version,
                prompt_digest,
                started_at,
            } => {
                if let Some(existing) = aggregate
                    .executions
                    .values()
                    .find(|execution| execution.started.run_id == run_id)
                {
                    if existing.started.execution_id == internal_execution_id
                        && existing.started.agent_instance_id == agent_instance_id
                        && existing.started.request_id == request_id
                        && existing.started.source_turn_id == source_turn_id
                        && existing.started.detached_recipient_agent_instance_id
                            == detached_recipient_agent_instance_id
                        && existing.started.prompt_assembly_version == prompt_assembly_version
                        && existing.started.prompt_digest == prompt_digest
                    {
                        return Ok(agent_ack(
                            session_id,
                            &agent_instance_id,
                            aggregate.revision,
                        ));
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                (
                    stable("execution-start", &[session_id, &run_id]),
                    agent_instance_id.clone(),
                    started_at,
                    vec![EventData::ExecutionStarted(ExecutionStartedV1 {
                        run_id,
                        execution_id: internal_execution_id,
                        request_id,
                        admitted_revision: aggregate.revision,
                        base_message_id: admitted_base(&aggregate, &agent_instance_id),
                        tree_base_entry_id: admitted_tree_base(&aggregate, &agent_instance_id),
                        agent_instance_id,
                        source_turn_id,
                        detached_recipient_agent_instance_id,
                        prompt_assembly_version,
                        prompt_digest,
                        started_at,
                    })],
                )
            }
            AgentDurableCommand::RunTerminal {
                run_id,
                report,
                finished_at,
            } => {
                let execution = aggregate
                    .executions
                    .values()
                    .find(|execution| execution.started.run_id == run_id)
                    .ok_or(CommitError::IdentityMismatch)?;
                if execution.report.as_ref() == Some(&report)
                    && execution.finished_at == Some(finished_at)
                {
                    return Ok(agent_ack(
                        session_id,
                        &report.agent_instance_id,
                        aggregate.revision,
                    ));
                }
                if execution.report.is_some() {
                    return Err(CommitError::IdempotencyConflict);
                }
                (
                    stable("execution-finish", &[session_id, &run_id]),
                    report.agent_instance_id.clone(),
                    finished_at,
                    vec![EventData::ExecutionFinished {
                        execution_id: execution.started.execution_id.clone(),
                        report,
                        finished_at,
                    }],
                )
            }
            AgentDurableCommand::InputQueued {
                agent_instance_id,
                queued_input,
            } => {
                if let Some(existing) = aggregate
                    .queued_inputs
                    .iter()
                    .find(|input| input.queued_input_id == queued_input.queued_input_id)
                {
                    if existing == &queued_input {
                        return Ok(agent_ack(
                            session_id,
                            &agent_instance_id,
                            aggregate.revision,
                        ));
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                let now = chrono::Utc::now().timestamp_millis();
                (
                    stable("input-queued", &[session_id, &queued_input.queued_input_id]),
                    agent_instance_id,
                    now,
                    vec![EventData::AgentInputQueued {
                        input: queued_input,
                    }],
                )
            }
            AgentDurableCommand::QueuedInputCancelled {
                agent_instance_id,
                queued_input_id,
                cancelled_at,
            } => {
                let Some(input) = aggregate
                    .queued_inputs
                    .iter()
                    .find(|input| input.queued_input_id == queued_input_id)
                else {
                    return Ok(agent_ack(
                        session_id,
                        &agent_instance_id,
                        aggregate.revision,
                    ));
                };
                if input.request.agent_instance_id != agent_instance_id {
                    return Err(CommitError::IdentityMismatch);
                }
                (
                    stable("input-cancel", &[session_id, &queued_input_id]),
                    agent_instance_id,
                    cancelled_at,
                    vec![EventData::AgentInputDequeued {
                        queued_input_id,
                        reason: "cancelled".into(),
                        dequeued_at: cancelled_at,
                    }],
                )
            }
            AgentDurableCommand::QueuedInputStarted {
                agent_instance_id,
                queued_input_id,
                run_id,
                internal_execution_id,
                request_id,
                source_turn_id,
                detached_recipient_agent_instance_id,
                prompt_assembly_version,
                prompt_digest,
                started_at,
            } => {
                if let Some(existing) = aggregate
                    .executions
                    .values()
                    .find(|execution| execution.started.run_id == run_id)
                {
                    if existing.started.execution_id == internal_execution_id
                        && existing.started.agent_instance_id == agent_instance_id
                        && existing.started.request_id == request_id
                        && existing.started.source_turn_id == source_turn_id
                        && existing.started.detached_recipient_agent_instance_id
                            == detached_recipient_agent_instance_id
                        && existing.started.prompt_assembly_version == prompt_assembly_version
                        && existing.started.prompt_digest == prompt_digest
                    {
                        return Ok(agent_ack(
                            session_id,
                            &agent_instance_id,
                            aggregate.revision,
                        ));
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                let queued = aggregate
                    .queued_inputs
                    .iter()
                    .find(|input| input.queued_input_id == queued_input_id)
                    .ok_or(CommitError::IdentityMismatch)?;
                if queued.request.agent_instance_id != agent_instance_id {
                    return Err(CommitError::IdentityMismatch);
                }
                (
                    stable("queued-execution-start", &[session_id, &run_id]),
                    agent_instance_id.clone(),
                    started_at,
                    vec![
                        EventData::AgentInputDequeued {
                            queued_input_id,
                            reason: "started".into(),
                            dequeued_at: started_at,
                        },
                        EventData::ExecutionStarted(ExecutionStartedV1 {
                            run_id,
                            execution_id: internal_execution_id,
                            request_id,
                            admitted_revision: aggregate.revision,
                            base_message_id: admitted_base(&aggregate, &agent_instance_id),
                            tree_base_entry_id: admitted_tree_base(&aggregate, &agent_instance_id),
                            agent_instance_id,
                            source_turn_id,
                            detached_recipient_agent_instance_id,
                            prompt_assembly_version,
                            prompt_digest,
                            started_at,
                        }),
                    ],
                )
            }
            AgentDurableCommand::CommitReport {
                recipient_agent_instance_id,
                report,
            } => {
                if let Some(existing) = aggregate.inbox.get(&report.report_id) {
                    if existing.recipient_agent_instance_id == recipient_agent_instance_id
                        && existing.report == report
                    {
                        return Ok(agent_ack(
                            session_id,
                            &recipient_agent_instance_id,
                            aggregate.revision,
                        ));
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                let now = chrono::Utc::now().timestamp_millis();
                (
                    stable(
                        "inbox-report",
                        &[session_id, &recipient_agent_instance_id, &report.report_id],
                    ),
                    recipient_agent_instance_id.clone(),
                    now,
                    vec![EventData::InboxReportCommitted {
                        item: AgentInboxItem {
                            report_id: report.report_id.clone(),
                            recipient_agent_instance_id,
                            source_agent_instance_id: report.agent_instance_id.clone(),
                            report,
                            committed_at: now,
                            consumed_at: None,
                        },
                    }],
                )
            }
            AgentDurableCommand::ConsumeInboxItem {
                agent_instance_id,
                report_id,
                consumed_at,
            } => {
                let item = aggregate
                    .inbox
                    .get(&report_id)
                    .ok_or(CommitError::IdentityMismatch)?;
                if item.recipient_agent_instance_id != agent_instance_id {
                    return Err(CommitError::IdentityMismatch);
                }
                if item.consumed_at == Some(consumed_at) {
                    return Ok(agent_ack(
                        session_id,
                        &agent_instance_id,
                        aggregate.revision,
                    ));
                }
                if item.consumed_at.is_some() {
                    return Err(CommitError::IdempotencyConflict);
                }
                (
                    stable(
                        "inbox-consume",
                        &[session_id, &agent_instance_id, &report_id],
                    ),
                    agent_instance_id.clone(),
                    consumed_at,
                    vec![EventData::InboxReportConsumed {
                        report_id,
                        recipient_agent_instance_id: agent_instance_id,
                        consumed_at,
                    }],
                )
            }
        };
        let revision = self
            .commit_events(&commit_id, committed_at, events)
            .map_err(Self::commit_error)?;
        Ok(agent_ack(session_id, &agent_instance_id, revision))
    }
}

fn stable(kind: &str, parts: &[&str]) -> String {
    piko_orchd_api::stable_internal_id(kind, parts)
}

fn admitted_base(
    aggregate: &piko_session_store::SessionAggregate,
    agent_instance_id: &str,
) -> Option<String> {
    if aggregate
        .root
        .as_ref()
        .is_some_and(|root| root.agent_instance_id == agent_instance_id)
    {
        aggregate
            .root_base_message_id
            .clone()
            .or_else(|| aggregate.agent_heads.get(agent_instance_id).cloned())
    } else {
        aggregate.agent_heads.get(agent_instance_id).cloned()
    }
}

fn admitted_tree_base(
    aggregate: &piko_session_store::SessionAggregate,
    agent_instance_id: &str,
) -> Option<String> {
    if aggregate
        .root
        .as_ref()
        .is_some_and(|root| root.agent_instance_id == agent_instance_id)
    {
        aggregate.selected_tree_entry_id.clone()
    } else {
        aggregate
            .agent_heads
            .get(agent_instance_id)
            .cloned()
            .or_else(|| aggregate.selected_tree_entry_id.clone())
    }
}

fn agent_ack(session_id: &str, agent_instance_id: &str, revision: u64) -> AgentCommitAck {
    AgentCommitAck {
        session_id: session_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        revision,
    }
}
