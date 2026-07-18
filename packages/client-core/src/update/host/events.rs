//! Live Turn, tool, queue, and prompt event projection.

use crate::state::{ActiveTurn, ClientState, TurnFailure};
use piko_protocol::TurnStatus;

use super::is_live_session_event;

pub(super) fn handle_turn_lifecycle(state: &mut ClientState, event: piko_protocol::TurnEvent) {
    let (session_id, turn_id, agent_instance_id) = match &event {
        piko_protocol::TurnEvent::Queued {
            session_id,
            turn_id,
            agent_instance_id,
            ..
        }
        | piko_protocol::TurnEvent::Started {
            session_id,
            turn_id,
            agent_instance_id,
            ..
        }
        | piko_protocol::TurnEvent::Completed {
            session_id,
            turn_id,
            agent_instance_id,
            ..
        }
        | piko_protocol::TurnEvent::Failed {
            session_id,
            turn_id,
            agent_instance_id,
            ..
        }
        | piko_protocol::TurnEvent::Cancelled {
            session_id,
            turn_id,
            agent_instance_id,
            ..
        } => (
            session_id.clone(),
            turn_id.clone(),
            agent_instance_id.clone(),
        ),
    };

    if !is_live_session_event(state, &session_id) {
        return;
    }

    let Some(session) = &mut state.live_session else {
        return;
    };

    let failure = match &event {
        piko_protocol::TurnEvent::Failed { error, .. } => Some(error.clone()),
        _ => None,
    };
    let status = match &event {
        piko_protocol::TurnEvent::Queued { .. } => TurnStatus::Queued,
        piko_protocol::TurnEvent::Started { .. } => TurnStatus::Running,
        piko_protocol::TurnEvent::Completed { .. } => TurnStatus::Completed,
        piko_protocol::TurnEvent::Failed { .. } => TurnStatus::Failed,
        piko_protocol::TurnEvent::Cancelled { .. } => TurnStatus::Cancelled,
    };

    let is_terminal = matches!(
        status,
        TurnStatus::Completed | TurnStatus::Failed | TurnStatus::Cancelled
    );

    if is_terminal {
        session.active_turns.retain(|t| t.turn_id != turn_id);
        if let Some(error) = failure {
            session.turn_failures.retain(|item| item.turn_id != turn_id);
            session.turn_failures.push(TurnFailure {
                turn_id,
                agent_instance_id,
                error,
            });
            const MAX_FAILURES: usize = 20;
            if session.turn_failures.len() > MAX_FAILURES {
                session
                    .turn_failures
                    .drain(..session.turn_failures.len() - MAX_FAILURES);
            }
        }
    } else if let Some(existing) = session
        .active_turns
        .iter_mut()
        .find(|t| t.turn_id == turn_id)
    {
        existing.status = status;
    } else {
        session.active_turns.push(ActiveTurn {
            turn_id,
            agent_instance_id,
            status,
        });
    }
}

pub(super) fn handle_tool_execution(
    state: &mut ClientState,
    event: piko_protocol::ToolExecutionEvent,
) {
    match event {
        piko_protocol::ToolExecutionEvent::Started {
            session_id,
            agent_instance_id,
            tool_call_id,
            tool_name,
            args,
            parent_message_id,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session {
                session
                    .timelines
                    .entry(agent_instance_id)
                    .or_default()
                    .apply_tool_started(tool_call_id, tool_name, args, parent_message_id);
            }
        }
        piko_protocol::ToolExecutionEvent::Ended {
            session_id,
            agent_instance_id,
            tool_call_id,
            tool_name,
            result,
            is_error,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session {
                session
                    .timelines
                    .entry(agent_instance_id)
                    .or_default()
                    .apply_tool_ended(tool_call_id, tool_name, result, is_error);
            }
        }
    }
}

pub(super) fn handle_queue_event(state: &mut ClientState, event: piko_protocol::QueueEvent) {
    let piko_protocol::QueueEvent::Updated {
        session_id,
        steer_count,
        follow_up_count,
        next_turn_count,
        steer_preview,
        follow_up_preview,
    } = event;
    if !is_live_session_event(state, &session_id) {
        return;
    }
    if let Some(session) = &mut state.live_session {
        session.queue = crate::state::QueueProjection {
            steer_count,
            follow_up_count,
            next_turn_count,
            steer_preview,
            follow_up_preview,
        };
    }
}

pub(super) fn handle_approval_event(state: &mut ClientState, event: piko_protocol::ApprovalEvent) {
    match event {
        piko_protocol::ApprovalEvent::Requested {
            session_id,
            agent_instance_id,
            approval_id,
            tool_name,
            tool_args,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session
                && !session
                    .pending_approvals
                    .iter()
                    .any(|a| a.approval_id == approval_id)
            {
                session
                    .pending_approvals
                    .push(crate::state::PendingApproval {
                        approval_id,
                        agent_instance_id,
                        tool_name,
                        tool_args,
                        response_in_flight: false,
                    });
            }
        }
        piko_protocol::ApprovalEvent::Resolved {
            session_id,
            approval_id,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session {
                session
                    .pending_approvals
                    .retain(|a| a.approval_id != approval_id);
            }
        }
    }
}

pub(super) fn handle_interaction_event(
    state: &mut ClientState,
    event: piko_protocol::InteractionEvent,
) {
    match event {
        piko_protocol::InteractionEvent::Requested {
            session_id,
            agent_instance_id,
            interaction_id,
            questions,
            require_confirm,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session
                && !session
                    .pending_interactions
                    .iter()
                    .any(|i| i.interaction_id == interaction_id)
            {
                session
                    .pending_interactions
                    .push(crate::state::PendingInteraction {
                        interaction_id,
                        agent_instance_id,
                        questions,
                        require_confirm,
                        response_in_flight: false,
                    });
            }
        }
        piko_protocol::InteractionEvent::Resolved {
            session_id,
            interaction_id,
            ..
        } => {
            if !is_live_session_event(state, &session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session {
                session
                    .pending_interactions
                    .retain(|i| i.interaction_id != interaction_id);
            }
        }
    }
}
