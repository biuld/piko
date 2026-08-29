//! Compatibility queue and execution-terminal command handling.

use piko_protocol::{AgentInput, AgentInputDisposition, AgentRunReport, CommitError};
use piko_session_store::{EventData, ExecutionStartedV1};

use super::{admitted_base, admitted_tree_base, stable};

pub(super) fn finish_run(
    aggregate: &piko_session_store::SessionAggregate,
    session_id: &str,
    run_id: String,
    report: AgentRunReport,
    finished_at: i64,
) -> Result<(String, String, i64, Vec<EventData>), CommitError> {
    let execution = aggregate
        .executions
        .values()
        .find(|execution| execution.started.run_id == run_id)
        .ok_or(CommitError::IdentityMismatch)?;
    if execution.report.as_ref() == Some(&report) && execution.finished_at == Some(finished_at) {
        return Ok((String::new(), report.agent_instance_id, 0, Vec::new()));
    }
    if execution.report.is_some() {
        return Err(CommitError::IdempotencyConflict);
    }
    let mut events = aggregate
        .agent_inputs
        .values()
        .filter(|input| {
            input.input.agent_instance_id == execution.started.agent_instance_id
                && input.disposition == AgentInputDisposition::PendingSteer
                && input.bound_run_id.as_deref() == Some(run_id.as_str())
        })
        .map(|input| {
            EventData::AgentInputDispositionChangedV1(
                piko_session_store::AgentInputDispositionChangedV1 {
                    agent_instance_id: execution.started.agent_instance_id.clone(),
                    input_id: input.input.input_id.clone(),
                    disposition: AgentInputDisposition::Cancelled,
                    root_input_id: input.root_input_id.clone(),
                    run_id: input.run_id.clone(),
                    bound_run_id: input.bound_run_id.clone(),
                    model_step_id: None,
                    changed_at: finished_at,
                },
            )
        })
        .collect::<Vec<_>>();
    events.push(EventData::ExecutionFinished {
        execution_id: execution.started.execution_id.clone(),
        report,
        finished_at,
    });
    Ok((
        stable("execution-finish", &[session_id, &run_id]),
        execution.started.agent_instance_id.clone(),
        finished_at,
        events,
    ))
}

pub(super) fn enqueue(
    aggregate: &piko_session_store::SessionAggregate,
    session_id: &str,
    agent_instance_id: String,
    queued_input: piko_protocol::DurableAgentInput,
) -> Result<(String, String, i64, Vec<EventData>), CommitError> {
    if queued_input.request.agent_instance_id != agent_instance_id {
        return Err(CommitError::IdentityMismatch);
    }
    if let Some(existing) = aggregate
        .queued_inputs
        .iter()
        .find(|input| input.queued_input_id == queued_input.queued_input_id)
    {
        if existing == &queued_input {
            return Ok((String::new(), agent_instance_id, 0, Vec::new()));
        }
        return Err(CommitError::IdempotencyConflict);
    }
    let now = chrono::Utc::now().timestamp_millis();
    let admitted_at = queued_input.submitted_at.unwrap_or(now);
    let mut canonical_input = AgentInput::from_request(&queued_input.request, admitted_at);
    canonical_input.input_id = queued_input.queued_input_id.clone();
    Ok((
        stable("input-queued", &[session_id, &queued_input.queued_input_id]),
        agent_instance_id,
        now,
        vec![
            EventData::AgentInputAdmittedV1(piko_session_store::AgentInputAdmittedV1 {
                input: canonical_input,
                disposition: AgentInputDisposition::PendingFollowUp,
                root_input_id: None,
                run_id: None,
                bound_run_id: None,
                admitted_at,
            }),
            EventData::AgentInputQueued {
                input: queued_input,
            },
        ],
    ))
}

pub(super) fn cancel(
    aggregate: &piko_session_store::SessionAggregate,
    session_id: &str,
    agent_instance_id: String,
    queued_input_id: String,
    cancelled_at: i64,
) -> Result<(String, String, i64, Vec<EventData>), CommitError> {
    let legacy_input = aggregate
        .queued_inputs
        .iter()
        .find(|input| input.queued_input_id == queued_input_id);
    let canonical_input = aggregate.agent_inputs.get(&queued_input_id);
    if legacy_input.is_none() && canonical_input.is_none() {
        return Ok((String::new(), agent_instance_id, 0, Vec::new()));
    }
    if legacy_input.is_some_and(|input| input.request.agent_instance_id != agent_instance_id)
        || canonical_input.is_some_and(|input| input.input.agent_instance_id != agent_instance_id)
    {
        return Err(CommitError::IdentityMismatch);
    }
    let mut events = Vec::new();
    if legacy_input.is_some() {
        events.push(EventData::AgentInputDequeued {
            queued_input_id: queued_input_id.clone(),
            reason: "cancelled".into(),
            dequeued_at: cancelled_at,
        });
    }
    if let Some(input) = canonical_input
        && input.disposition == AgentInputDisposition::PendingFollowUp
    {
        events.insert(
            0,
            EventData::AgentInputDispositionChangedV1(
                piko_session_store::AgentInputDispositionChangedV1 {
                    agent_instance_id: agent_instance_id.clone(),
                    input_id: queued_input_id.clone(),
                    disposition: AgentInputDisposition::Cancelled,
                    root_input_id: input.root_input_id.clone(),
                    run_id: input.run_id.clone(),
                    bound_run_id: input.bound_run_id.clone(),
                    model_step_id: None,
                    changed_at: cancelled_at,
                },
            ),
        );
    }
    Ok((
        stable("input-cancel", &[session_id, &queued_input_id]),
        agent_instance_id,
        cancelled_at,
        events,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn start_queued(
    aggregate: &piko_session_store::SessionAggregate,
    session_id: &str,
    agent_instance_id: String,
    queued_input_id: String,
    run_id: String,
    internal_execution_id: String,
    request_id: String,
    source_turn_id: Option<String>,
    detached_recipient_agent_instance_id: Option<String>,
    prompt_assembly_version: u32,
    prompt_digest: String,
    started_at: i64,
) -> Result<(String, String, i64, Vec<EventData>), CommitError> {
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
            return Ok((String::new(), agent_instance_id, 0, Vec::new()));
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
    let execution_event = EventData::ExecutionStarted(ExecutionStartedV1 {
        run_id: run_id.clone(),
        execution_id: internal_execution_id.clone(),
        request_id: request_id.clone(),
        admitted_revision: aggregate.revision,
        base_message_id: admitted_base(aggregate, &agent_instance_id),
        tree_base_entry_id: admitted_tree_base(aggregate, &agent_instance_id),
        agent_instance_id: agent_instance_id.clone(),
        source_turn_id: source_turn_id.clone(),
        detached_recipient_agent_instance_id: detached_recipient_agent_instance_id.clone(),
        prompt_assembly_version,
        prompt_digest: prompt_digest.clone(),
        started_at,
    });
    let mut events = Vec::with_capacity(3);
    if aggregate.agent_inputs.contains_key(&queued_input_id) {
        events.push(EventData::AgentInputDispositionChangedV1(
            piko_session_store::AgentInputDispositionChangedV1 {
                agent_instance_id: agent_instance_id.clone(),
                input_id: queued_input_id.clone(),
                disposition: AgentInputDisposition::AppliedAsRoot,
                root_input_id: Some(queued_input_id.clone()),
                run_id: Some(run_id.clone()),
                bound_run_id: None,
                model_step_id: None,
                changed_at: started_at,
            },
        ));
    } else {
        let mut canonical_input = AgentInput::from_request(&queued.request, started_at);
        canonical_input.input_id = queued_input_id.clone();
        events.push(EventData::AgentInputAdmittedV1(
            piko_session_store::AgentInputAdmittedV1 {
                input: canonical_input,
                disposition: AgentInputDisposition::AppliedAsRoot,
                root_input_id: None,
                run_id: Some(run_id.clone()),
                bound_run_id: None,
                admitted_at: started_at,
            },
        ));
    }
    events.push(EventData::AgentInputDequeued {
        queued_input_id,
        reason: "started".into(),
        dequeued_at: started_at,
    });
    events.push(execution_event);
    Ok((
        stable("queued-execution-start", &[session_id, &run_id]),
        agent_instance_id,
        started_at,
        events,
    ))
}
