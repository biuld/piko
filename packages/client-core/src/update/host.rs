//! Host (ServerMessage) handling for the update reducer.

mod events;

use piko_protocol::{Command, CommandResult, ServerMessage};

use crate::effect::ClientEffect;
use crate::state::{ClientState, LiveSession, PendingOp, SessionPhase};
use crate::timeline::ApplyOutcome;
use crate::update::UpdateContext;

pub(super) fn handle_host(
    state: &mut ClientState,
    msg: ServerMessage,
    ctx: &mut UpdateContext<'_>,
    effects: &mut Vec<ClientEffect>,
) {
    match msg {
        ServerMessage::CommandResponse { command_id, result } => {
            handle_command_response(state, &command_id, result);
        }
        ServerMessage::SessionReconciled(event) => {
            handle_session_reconciled(state, event);
        }
        ServerMessage::SessionCleared(event) => {
            handle_session_cleared(state, &event.previous_session_id);
        }
        ServerMessage::TranscriptCommitted(event) => {
            if !is_live_session_event(state, &event.session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session {
                let timeline = session
                    .timelines
                    .entry(event.agent_instance_id.clone())
                    .or_default();
                let outcome = timeline.apply_committed_checked(
                    event.message_id,
                    event.transcript_seq,
                    event.message,
                    event.source_turn_id,
                );
                if outcome == ApplyOutcome::Inconsistent {
                    request_refresh(state, ctx, effects);
                }
            }
        }
        ServerMessage::RealtimeMessage(event) => {
            if !is_live_session_event(state, &event.session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session {
                let timeline = session
                    .timelines
                    .entry(event.agent_instance_id.clone())
                    .or_default();
                let outcome = timeline.apply_realtime_checked(
                    event.message_id,
                    event.delta_seq,
                    &event.delta,
                );
                if outcome == ApplyOutcome::Inconsistent {
                    request_refresh(state, ctx, effects);
                }
            }
        }
        ServerMessage::TurnLifecycle(event) => {
            events::handle_turn_lifecycle(state, event);
        }
        ServerMessage::Approval(event) => {
            events::handle_approval_event(state, event);
        }
        ServerMessage::Interaction(event) => {
            events::handle_interaction_event(state, event);
        }
        ServerMessage::ToolExecution(event) => {
            events::handle_tool_execution(state, event);
        }
        ServerMessage::Queue(event) => {
            events::handle_queue_event(state, event);
        }
        ServerMessage::AgentChanged(info) => {
            if !is_live_session_event(state, &info.session_id) {
                return;
            }
            if let Some(session) = &mut state.live_session {
                if let Some(existing) = session
                    .agents
                    .iter_mut()
                    .find(|a| a.agent_instance_id == info.agent_instance_id)
                {
                    *existing = info;
                } else {
                    session.agents.push(info);
                }
            }
        }
        ServerMessage::Model(model_event) => {
            let piko_protocol::ModelEvent::ConfigChanged {
                model_id,
                provider,
                thinking_level,
                ..
            } = model_event;
            state.model.model_id = Some(model_id);
            state.model.provider = Some(provider);
            if let Some(level) = thinking_level {
                state.model.thinking_level = Some(level.as_str().to_string());
            }
            let current_provider = state.model.provider.as_deref();
            let current_model = state.model.model_id.as_deref();
            let current_thinking = state.model.thinking_level.as_deref();
            state.pending_commands.retain(|_, op| match op {
                PendingOp::SetModel { provider, model_id } => {
                    current_provider != Some(provider.as_str())
                        || current_model != Some(model_id.as_str())
                }
                PendingOp::SetThinkingLevel { level } => current_thinking != Some(level.as_str()),
                _ => true,
            });
        }
        _ => {}
    }
}

fn request_refresh(
    state: &mut ClientState,
    ctx: &mut UpdateContext<'_>,
    effects: &mut Vec<ClientEffect>,
) {
    if state
        .pending_commands
        .values()
        .any(|op| matches!(op, PendingOp::Refresh))
    {
        return;
    }
    let Some(session_id) = state.live_session_id().map(str::to_string) else {
        return;
    };
    let command_id = ctx.command_ids.next_command_id();
    state
        .pending_commands
        .insert(command_id.clone(), PendingOp::Refresh);
    effects.push(ClientEffect::Send(Command::StateSnapshot {
        command_id,
        session_id,
    }));
}

fn handle_command_response(
    state: &mut ClientState,
    command_id: &str,
    result: Result<CommandResult, String>,
) {
    let Some(op) = state.pending_commands.remove(command_id) else {
        return;
    };

    match result {
        Ok(cmd_result) => match (op, cmd_result) {
            (PendingOp::Discover, CommandResult::SessionListed { sessions, .. }) => {
                state.session_list.sessions = sessions;
            }
            (
                PendingOp::Open { session_id },
                CommandResult::SessionOpened {
                    session_id: opened_id,
                    ..
                },
            ) => {
                if opened_id == session_id {
                    state.session_phase = SessionPhase::Hydrating {
                        target_id: session_id,
                    };
                } else {
                    state.last_error = Some(format!(
                        "host opened unexpected session {opened_id}; expected {session_id}"
                    ));
                    state.session_phase = if state.live_session.is_some() {
                        SessionPhase::Live
                    } else {
                        SessionPhase::IdleNoSession
                    };
                }
            }
            (PendingOp::Create, CommandResult::SessionCreated { session_id, .. }) => {
                state.session_phase = SessionPhase::Hydrating {
                    target_id: session_id,
                };
            }
            (
                PendingOp::SelectAgent { agent_instance_id },
                CommandResult::AgentSubscribed {
                    session_id,
                    agent_instance_id: subscribed_agent,
                    snapshot,
                    replay,
                    ..
                },
            ) => {
                if !is_live_session_event(state, &session_id)
                    || subscribed_agent != agent_instance_id
                {
                    state.last_error = Some("host returned mismatched Agent subscription".into());
                    return;
                }
                if let Some(session) = &mut state.live_session {
                    session.selected_agent = Some(agent_instance_id.clone());
                    let timeline = session.timelines.entry(agent_instance_id).or_default();
                    timeline.clear();
                    let events = if !snapshot.events.is_empty() {
                        &snapshot.events
                    } else {
                        &replay
                    };
                    for seq_msg in events {
                        apply_sequenced_to_timeline(
                            timeline,
                            &seq_msg.message,
                            &session.session_id,
                        );
                    }
                }
            }
            (PendingOp::Refresh, CommandResult::Empty)
            | (PendingOp::Delete { .. }, CommandResult::Empty)
            | (PendingOp::Submit, CommandResult::Empty)
            | (PendingOp::Cancel, CommandResult::Empty)
            | (PendingOp::ApprovalRespond { .. }, CommandResult::Empty)
            | (PendingOp::InteractionRespond { .. }, CommandResult::Empty) => {}
            (PendingOp::Navigate { .. }, CommandResult::SessionNavigated { .. }) => {}
            (PendingOp::ListModels, CommandResult::ModelListed { providers, .. }) => {
                state.model.providers = providers;
            }
            _ => {}
        },
        Err(error_msg) => {
            state.last_error = Some(error_msg.clone());
            state.command_failures.push(crate::state::CommandFailure {
                command_id: command_id.to_string(),
                operation: op.clone(),
                message: error_msg,
            });
            const MAX_COMMAND_FAILURES: usize = 50;
            if state.command_failures.len() > MAX_COMMAND_FAILURES {
                state
                    .command_failures
                    .drain(..state.command_failures.len() - MAX_COMMAND_FAILURES);
            }
            match op {
                PendingOp::Open { .. } | PendingOp::Create => {
                    if state.live_session.is_some() {
                        state.session_phase = SessionPhase::Live;
                    } else {
                        state.take_previous_live();
                        state.session_phase = SessionPhase::IdleNoSession;
                    }
                }
                PendingOp::ApprovalRespond { approval_id } => {
                    if let Some(session) = &mut state.live_session
                        && let Some(prompt) = session
                            .pending_approvals
                            .iter_mut()
                            .find(|prompt| prompt.approval_id == approval_id)
                    {
                        prompt.response_in_flight = false;
                    }
                }
                PendingOp::InteractionRespond { interaction_id } => {
                    if let Some(session) = &mut state.live_session
                        && let Some(prompt) = session
                            .pending_interactions
                            .iter_mut()
                            .find(|prompt| prompt.interaction_id == interaction_id)
                    {
                        prompt.response_in_flight = false;
                    }
                }
                _ => {}
            }
        }
    }
}

fn handle_session_reconciled(
    state: &mut ClientState,
    event: piko_protocol::SessionReconciledEvent,
) {
    let accept = match &state.session_phase {
        SessionPhase::Hydrating { target_id } => *target_id == event.session_id,
        SessionPhase::Live => state.live_session_id() == Some(event.session_id.as_str()),
        _ => false,
    };
    if !accept {
        return;
    }

    let session = LiveSession::from_reconcile(event.session_id, &event.snapshot, &event.agents);
    state.live_session = Some(session);
    state.session_phase = SessionPhase::Live;
}

fn handle_session_cleared(state: &mut ClientState, previous_session_id: &str) {
    if state.live_session_id() == Some(previous_session_id) {
        state.live_session = None;
        state.session_phase = SessionPhase::IdleNoSession;
    }
}

pub(super) fn is_live_session_event(state: &ClientState, session_id: &str) -> bool {
    state.session_phase == SessionPhase::Live && state.live_session_id() == Some(session_id)
}

fn apply_sequenced_to_timeline(
    timeline: &mut crate::timeline::AgentTimeline,
    msg: &ServerMessage,
    expected_session_id: &str,
) {
    match msg {
        ServerMessage::TranscriptCommitted(event) if event.session_id == expected_session_id => {
            timeline.apply_committed(
                event.message_id.clone(),
                event.transcript_seq,
                event.message.clone(),
                event.source_turn_id.clone(),
            );
        }
        ServerMessage::RealtimeMessage(event) if event.session_id == expected_session_id => {
            timeline.apply_realtime(event.message_id.clone(), event.delta_seq, &event.delta);
        }
        _ => {}
    }
}
