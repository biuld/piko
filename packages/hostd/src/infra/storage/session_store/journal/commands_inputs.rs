//! Durable admission and root-work command handling.

use piko_protocol::{AgentDurableCommand, AgentInputDisposition, CommitError};
use piko_session_store::{AgentInputProcessingStartedV1, EventData};

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
    let model_step_id = change
        .model_step_id
        .clone()
        .or_else(|| existing.model_step_id.clone());
    if existing.disposition == disposition
        && existing.root_input_id == root_input_id
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
    let AgentDurableCommand::AgentInputProcessingStarted {
        agent_instance_id,
        root_input_id,
        request_id,
        source_turn_id,
        detached_recipient_agent_instance_id,
        prompt_assembly_version,
        prompt_digest,
        started_at,
        input,
    } = command
    else {
        unreachable!("root work helper called for another command")
    };
    if input.session_id != session_id
        || input.agent_instance_id != agent_instance_id
        || input.request_id != request_id
        || input.input_id != root_input_id
        || root_input_id.is_empty()
    {
        return Err(CommitError::IdentityMismatch);
    }
    let committed_input_id = root_input_id.clone();
    if let Some(existing) = aggregate.agent_inputs.get(&root_input_id) {
        if let Some(processing) = &existing.processing {
            let matches = existing.input == input
                && processing.started_at == started_at
                && processing.source_turn_id == source_turn_id
                && processing.detached_recipient_agent_instance_id
                    == detached_recipient_agent_instance_id
                && processing.prompt_assembly_version == prompt_assembly_version
                && processing.prompt_digest == prompt_digest;
            return if matches {
                Ok((String::new(), agent_instance_id, 0, Vec::new()))
            } else {
                Err(CommitError::IdempotencyConflict)
            };
        }
        if existing.input != input {
            return Err(CommitError::IdempotencyConflict);
        }
    }
    let processing_event =
        EventData::AgentInputProcessingStartedV1(AgentInputProcessingStartedV1 {
            agent_instance_id: agent_instance_id.clone(),
            root_input_id: root_input_id.clone(),
            request_id,
            base_message_id: admitted_base(aggregate, &agent_instance_id),
            tree_base_entry_id: admitted_tree_base(aggregate, &agent_instance_id),
            source_turn_id,
            detached_recipient_agent_instance_id,
            prompt_assembly_version,
            prompt_digest,
            started_at,
        });
    let mut events = Vec::with_capacity(2);
    match aggregate.agent_inputs.get(&root_input_id) {
        None => events.push(EventData::AgentInputAdmittedV1(
            piko_session_store::AgentInputAdmittedV1 {
                input,
                disposition: AgentInputDisposition::AppliedAsRoot,
                root_input_id: None,
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
            events.push(EventData::AgentInputDispositionChangedV1(
                piko_session_store::AgentInputDispositionChangedV1 {
                    agent_instance_id: agent_instance_id.clone(),
                    input_id: root_input_id.clone(),
                    disposition: AgentInputDisposition::AppliedAsRoot,
                    root_input_id: Some(root_input_id),
                    model_step_id: None,
                    changed_at: started_at,
                },
            ));
        }
        Some(_) => return Err(CommitError::IdempotencyConflict),
    }
    events.push(processing_event);
    Ok((
        stable("work-start", &[session_id, &committed_input_id]),
        agent_instance_id,
        started_at,
        events,
    ))
}
