//! Execution-terminal command handling.

use piko_protocol::{AgentInputDisposition, AgentWorkReport, CommitError};
use piko_session_store::EventData;

use super::stable;

pub(super) fn finish_run(
    aggregate: &piko_session_store::SessionAggregate,
    session_id: &str,
    run_id: String,
    report: AgentWorkReport,
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
    let root_input_id = aggregate
        .agent_inputs
        .values()
        .find(|input| input.input.request_id == execution.started.request_id)
        .and_then(|input| input.root_input_id.as_deref())
        .ok_or(CommitError::IdentityMismatch)?;
    let mut events = aggregate
        .agent_inputs
        .values()
        .filter(|input| {
            input.input.agent_instance_id == execution.started.agent_instance_id
                && input.disposition == AgentInputDisposition::PendingSteer
                && input.root_input_id.as_deref() == Some(root_input_id)
        })
        .map(|input| {
            EventData::AgentInputDispositionChangedV1(
                piko_session_store::AgentInputDispositionChangedV1 {
                    agent_instance_id: execution.started.agent_instance_id.clone(),
                    input_id: input.input.input_id.clone(),
                    disposition: AgentInputDisposition::Cancelled,
                    root_input_id: input.root_input_id.clone(),
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
