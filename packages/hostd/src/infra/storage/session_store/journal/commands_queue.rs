//! Root-work terminal command handling.

use piko_protocol::{AgentInputDisposition, AgentWorkReport, CommitError};
use piko_session_store::{AgentInputProcessingFinishedV1, EventData};

use super::stable;

pub(super) fn finish_run(
    aggregate: &piko_session_store::SessionAggregate,
    session_id: &str,
    agent_instance_id: String,
    root_input_id: String,
    report: AgentWorkReport,
    finished_at: i64,
) -> Result<(String, String, i64, Vec<EventData>), CommitError> {
    let stored = aggregate
        .agent_inputs
        .get(&root_input_id)
        .ok_or(CommitError::IdentityMismatch)?;
    if stored.input.agent_instance_id != agent_instance_id
        || stored.input.agent_instance_id != report.agent_instance_id
        || report.root_input_id != root_input_id
    {
        return Err(CommitError::IdentityMismatch);
    }
    let processing = stored
        .processing
        .as_ref()
        .ok_or(CommitError::IdentityMismatch)?;
    if processing.report.as_ref() == Some(&report) && processing.finished_at == Some(finished_at) {
        return Ok((String::new(), agent_instance_id, 0, Vec::new()));
    }
    if processing.finished_at.is_some() {
        return Err(CommitError::IdempotencyConflict);
    }
    let mut events = aggregate
        .agent_inputs
        .values()
        .filter(|input| {
            input.input.agent_instance_id == agent_instance_id
                && input.disposition == AgentInputDisposition::PendingSteer
                && input.root_input_id.as_deref() == Some(root_input_id.as_str())
        })
        .map(|input| {
            EventData::AgentInputDispositionChangedV1(
                piko_session_store::AgentInputDispositionChangedV1 {
                    agent_instance_id: agent_instance_id.clone(),
                    input_id: input.input.input_id.clone(),
                    disposition: AgentInputDisposition::Cancelled,
                    root_input_id: input.root_input_id.clone(),
                    model_step_id: None,
                    changed_at: finished_at,
                },
            )
        })
        .collect::<Vec<_>>();
    events.push(EventData::AgentInputProcessingFinishedV1(
        AgentInputProcessingFinishedV1 {
            agent_instance_id,
            root_input_id,
            report,
            finished_at,
        },
    ));
    Ok((
        stable("work-finish", &[session_id, &stored.input.input_id]),
        stored.input.agent_instance_id.clone(),
        finished_at,
        events,
    ))
}
