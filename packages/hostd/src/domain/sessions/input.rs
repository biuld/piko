use crate::api::{ServerMessage, TurnStatus};
use crate::util::now_ms;

/// Map an admitted AgentInput disposition to the leftover TurnLifecycle status
/// still emitted on the wire. This is not stored.
pub fn turn_status_from_disposition(
    disposition: piko_protocol::AgentInputDisposition,
) -> TurnStatus {
    match disposition {
        piko_protocol::AgentInputDisposition::AppliedAsRoot
        | piko_protocol::AgentInputDisposition::AppliedToStep
        | piko_protocol::AgentInputDisposition::PendingSteer => TurnStatus::Running,
        piko_protocol::AgentInputDisposition::PendingFollowUp => TurnStatus::Queued,
        piko_protocol::AgentInputDisposition::Cancelled => TurnStatus::Cancelled,
    }
}

pub fn turn_started(
    session_id: impl Into<String>,
    root_input_id: impl Into<String>,
    agent_instance_id: impl Into<String>,
) -> ServerMessage {
    ServerMessage::TurnLifecycle(crate::api::TurnEvent::Started {
        session_id: session_id.into(),
        turn_id: root_input_id.into(),
        agent_instance_id: agent_instance_id.into(),
        timestamp: now_ms(),
    })
}

pub fn turn_queued(
    session_id: impl Into<String>,
    root_input_id: impl Into<String>,
    agent_instance_id: impl Into<String>,
) -> ServerMessage {
    ServerMessage::TurnLifecycle(crate::api::TurnEvent::Queued {
        session_id: session_id.into(),
        turn_id: root_input_id.into(),
        agent_instance_id: agent_instance_id.into(),
        timestamp: now_ms(),
    })
}

pub fn turn_completed(
    session_id: impl Into<String>,
    root_input_id: impl Into<String>,
    agent_instance_id: impl Into<String>,
    usage: piko_protocol::Usage,
) -> ServerMessage {
    crate::telemetry::handle().record_turn_usage(&usage, "completed");
    ServerMessage::TurnLifecycle(crate::api::TurnEvent::Completed {
        session_id: session_id.into(),
        turn_id: root_input_id.into(),
        agent_instance_id: agent_instance_id.into(),
        usage,
        timestamp: now_ms(),
    })
}

pub fn turn_failed(
    session_id: impl Into<String>,
    root_input_id: impl Into<String>,
    agent_instance_id: impl Into<String>,
    error: impl Into<String>,
    usage: piko_protocol::Usage,
) -> ServerMessage {
    crate::telemetry::handle().record_turn_usage(&usage, "failed");
    ServerMessage::TurnLifecycle(crate::api::TurnEvent::Failed {
        session_id: session_id.into(),
        turn_id: root_input_id.into(),
        agent_instance_id: agent_instance_id.into(),
        error: error.into(),
        usage,
        timestamp: now_ms(),
    })
}

pub fn turn_cancelled(
    session_id: impl Into<String>,
    root_input_id: impl Into<String>,
    agent_instance_id: impl Into<String>,
    usage: piko_protocol::Usage,
) -> ServerMessage {
    crate::telemetry::handle().record_turn_usage(&usage, "cancelled");
    ServerMessage::TurnLifecycle(crate::api::TurnEvent::Cancelled {
        session_id: session_id.into(),
        turn_id: root_input_id.into(),
        agent_instance_id: agent_instance_id.into(),
        usage,
        timestamp: now_ms(),
    })
}

pub fn turn_terminal_from_report(
    session_id: &str,
    root_input_id: &str,
    report: &piko_protocol::AgentWorkReport,
) -> ServerMessage {
    match &report.outcome {
        piko_protocol::ExecutionOutcome::Failed { error } => turn_failed(
            session_id,
            root_input_id,
            report.agent_instance_id.clone(),
            error.clone(),
            report.usage.clone(),
        ),
        piko_protocol::ExecutionOutcome::Cancelled { .. } => turn_cancelled(
            session_id,
            root_input_id,
            report.agent_instance_id.clone(),
            report.usage.clone(),
        ),
        piko_protocol::ExecutionOutcome::Succeeded { .. } => turn_completed(
            session_id,
            root_input_id,
            report.agent_instance_id.clone(),
            report.usage.clone(),
        ),
    }
}
