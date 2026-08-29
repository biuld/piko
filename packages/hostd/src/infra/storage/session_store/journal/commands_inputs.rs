//! Durable admission and root-run command handling.

use piko_protocol::{AgentDurableCommand, AgentInputDisposition, CommitError};
use piko_session_store::{EventData, ExecutionStartedV1};

use super::{admitted_base, admitted_tree_base, canonical_disposition, stable};

pub(super) fn admit(
    aggregate: &piko_session_store::SessionAggregate,
    session_id: &str,
    admission: piko_protocol::AgentInputAdmission,
) -> Result<(String, String, i64, Vec<EventData>), CommitError> {
    let input = admission.input;
    if input.session_id != session_id {
        return Err(CommitError::IdentityMismatch);
    }
    let disposition =
        canonical_disposition(admission.disposition).ok_or(CommitError::IdempotencyConflict)?;
    if matches!(
        disposition,
        AgentInputDisposition::AppliedToStep | AgentInputDisposition::Cancelled
    ) {
        return Err(CommitError::IdempotencyConflict);
    }
    let expected_root_input_id = if disposition == AgentInputDisposition::AppliedAsRoot {
        Some(input.input_id.clone())
    } else {
        admission.root_input_id.clone()
    };
    if let Some(existing) = aggregate.agent_inputs.get(&input.input_id) {
        if existing.input == input
            && existing.admission_disposition == disposition
            && existing.admission_root_input_id == expected_root_input_id
            && existing.admission_run_id == admission.run_id
            && existing.admission_bound_run_id == admission.bound_run_id
        {
            return Ok((String::new(), input.agent_instance_id, 0, Vec::new()));
        }
        return Err(CommitError::IdempotencyConflict);
    }
    if aggregate
        .agent_inputs
        .values()
        .any(|existing| existing.input.request_id == input.request_id)
    {
        return Err(CommitError::IdempotencyConflict);
    }
    Ok((
        stable("input-admitted", &[session_id, &input.input_id]),
        input.agent_instance_id.clone(),
        admission.admitted_at,
        vec![EventData::AgentInputAdmittedV1(
            piko_session_store::AgentInputAdmittedV1 {
                input,
                disposition,
                root_input_id: expected_root_input_id,
                run_id: admission.run_id,
                bound_run_id: admission.bound_run_id,
                admitted_at: admission.admitted_at,
            },
        )],
    ))
}

pub(super) fn change_disposition(
    aggregate: &piko_session_store::SessionAggregate,
    session_id: &str,
    change: piko_protocol::AgentInputDispositionChange,
) -> Result<(String, String, i64, Vec<EventData>), CommitError> {
    let disposition =
        canonical_disposition(change.disposition).ok_or(CommitError::IdempotencyConflict)?;
    let existing = aggregate
        .agent_inputs
        .get(&change.input_id)
        .ok_or(CommitError::IdentityMismatch)?;
    if existing.input.agent_instance_id != change.agent_instance_id {
        return Err(CommitError::IdentityMismatch);
    }
    let root_input_id = if disposition == AgentInputDisposition::AppliedAsRoot {
        Some(change.input_id.clone())
    } else {
        change
            .root_input_id
            .clone()
            .or_else(|| existing.root_input_id.clone())
    };
    let run_id = change.run_id.clone().or_else(|| existing.run_id.clone());
    let bound_run_id = change
        .bound_run_id
        .clone()
        .or_else(|| existing.bound_run_id.clone());
    let model_step_id = change
        .model_step_id
        .clone()
        .or_else(|| existing.model_step_id.clone());
    if existing.disposition == disposition
        && existing.root_input_id == root_input_id
        && existing.run_id == run_id
        && existing.bound_run_id == bound_run_id
        && existing.model_step_id == model_step_id
    {
        return Ok((String::new(), change.agent_instance_id, 0, Vec::new()));
    }
    Ok((
        stable(
            "input-disposition",
            &[session_id, &change.input_id, &format!("{disposition:?}")],
        ),
        change.agent_instance_id.clone(),
        change.changed_at,
        vec![EventData::AgentInputDispositionChangedV1(
            piko_session_store::AgentInputDispositionChangedV1 {
                agent_instance_id: change.agent_instance_id,
                input_id: change.input_id,
                disposition,
                root_input_id,
                run_id,
                bound_run_id,
                model_step_id,
                changed_at: change.changed_at,
            },
        )],
    ))
}

pub(super) fn start_run(
    aggregate: &piko_session_store::SessionAggregate,
    session_id: &str,
    command: AgentDurableCommand,
) -> Result<(String, String, i64, Vec<EventData>), CommitError> {
    let AgentDurableCommand::RunStarted {
        agent_instance_id,
        run_id,
        internal_execution_id,
        request_id,
        source_turn_id,
        detached_recipient_agent_instance_id,
        prompt_assembly_version,
        prompt_digest,
        started_at,
        input,
    } = command
    else {
        unreachable!("root run helper called for another command")
    };
    if let Some(input) = input.as_ref()
        && (input.session_id != session_id
            || input.agent_instance_id != agent_instance_id
            || input.request_id != request_id
            || input.input_id.is_empty())
    {
        return Err(CommitError::IdentityMismatch);
    }
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
            && input.as_ref().is_none_or(|input| {
                aggregate
                    .agent_inputs
                    .get(&input.input_id)
                    .is_some_and(|stored| stored.input == *input)
            })
        {
            return Ok((String::new(), agent_instance_id, 0, Vec::new()));
        }
        return Err(CommitError::IdempotencyConflict);
    }
    let committed_run_id = run_id.clone();
    let execution_event = EventData::ExecutionStarted(ExecutionStartedV1 {
        run_id,
        execution_id: internal_execution_id,
        request_id,
        admitted_revision: aggregate.revision,
        base_message_id: admitted_base(aggregate, &agent_instance_id),
        tree_base_entry_id: admitted_tree_base(aggregate, &agent_instance_id),
        agent_instance_id: agent_instance_id.clone(),
        source_turn_id,
        detached_recipient_agent_instance_id,
        prompt_assembly_version,
        prompt_digest,
        started_at,
    });
    let mut events = Vec::with_capacity(2);
    if let Some(input) = input {
        match aggregate.agent_inputs.get(&input.input_id) {
            None => events.push(EventData::AgentInputAdmittedV1(
                piko_session_store::AgentInputAdmittedV1 {
                    input,
                    disposition: AgentInputDisposition::AppliedAsRoot,
                    root_input_id: None,
                    run_id: Some(committed_run_id.clone()),
                    bound_run_id: None,
                    admitted_at: started_at,
                },
            )),
            Some(existing)
                if existing.input == input
                    && existing.disposition == AgentInputDisposition::AppliedAsRoot => {}
            Some(existing)
                if existing.input == input
                    && existing.disposition == AgentInputDisposition::PendingFollowUp =>
            {
                let input_id = input.input_id.clone();
                events.push(EventData::AgentInputDispositionChangedV1(
                    piko_session_store::AgentInputDispositionChangedV1 {
                        agent_instance_id: agent_instance_id.clone(),
                        input_id: input_id.clone(),
                        disposition: AgentInputDisposition::AppliedAsRoot,
                        root_input_id: Some(input_id),
                        run_id: Some(committed_run_id.clone()),
                        bound_run_id: None,
                        model_step_id: None,
                        changed_at: started_at,
                    },
                ));
            }
            Some(_) => return Err(CommitError::IdempotencyConflict),
        }
    }
    events.push(execution_event);
    Ok((
        stable("execution-start", &[session_id, &committed_run_id]),
        agent_instance_id,
        started_at,
        events,
    ))
}
