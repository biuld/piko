//! Elm-ish update boundary: `(state, msg, ctx) -> (state, effects)`.

mod host;

use piko_protocol::Command;

use crate::effect::ClientEffect;
use crate::intent::ClientIntent;
use crate::msg::{ClientMsg, TransportObservation};
use crate::state::{ClientState, ConnectionState, PendingOp, SessionPhase};

/// Allocates deterministic command ids for reducer tests and adapters.
pub trait CommandIdSource {
    fn next_command_id(&mut self) -> String;
}

/// Context injected into update. Must not call wall-clock or UUID APIs inside
/// the reducer itself.
pub struct UpdateContext<'a> {
    pub command_ids: &'a mut dyn CommandIdSource,
}

/// Apply one message to state, producing new state and effects.
pub fn update(
    mut state: ClientState,
    msg: ClientMsg,
    ctx: &mut UpdateContext<'_>,
) -> (ClientState, Vec<ClientEffect>) {
    let mut effects = Vec::new();

    match msg {
        ClientMsg::Intent(intent) => {
            handle_intent(&mut state, intent, ctx, &mut effects);
        }
        ClientMsg::Host(server_msg) => {
            host::handle_host(&mut state, *server_msg, ctx, &mut effects);
        }
        ClientMsg::Transport(obs) => {
            handle_transport(&mut state, obs);
        }
    }

    (state, effects)
}

fn handle_intent(
    state: &mut ClientState,
    intent: ClientIntent,
    ctx: &mut UpdateContext<'_>,
    effects: &mut Vec<ClientEffect>,
) {
    match intent {
        ClientIntent::DiscoverSessions { scope, cwd } => {
            let id = ctx.command_ids.next_command_id();
            state
                .pending_commands
                .insert(id.clone(), PendingOp::Discover);
            effects.push(ClientEffect::Send(Command::SessionList {
                command_id: id,
                scope,
                cwd,
            }));
        }
        ClientIntent::OpenSession {
            session_id,
            session_path,
        } => {
            state.save_previous_live();
            state.session_phase = SessionPhase::OpeningOrCreating {
                target_id: Some(session_id.clone()),
            };
            let id = ctx.command_ids.next_command_id();
            state.pending_commands.insert(
                id.clone(),
                PendingOp::Open {
                    session_id: session_id.clone(),
                },
            );
            effects.push(ClientEffect::Send(Command::SessionOpen {
                command_id: id,
                session_id,
                session_path,
            }));
        }
        ClientIntent::CreateSession { cwd } => {
            state.save_previous_live();
            state.session_phase = SessionPhase::OpeningOrCreating { target_id: None };
            let id = ctx.command_ids.next_command_id();
            state.pending_commands.insert(id.clone(), PendingOp::Create);
            effects.push(ClientEffect::Send(Command::SessionCreate {
                command_id: id,
                cwd,
            }));
        }
        ClientIntent::RefreshSession => {
            if let Some(session) = &state.live_session {
                let sid = session.session_id.clone();
                let id = ctx.command_ids.next_command_id();
                state
                    .pending_commands
                    .insert(id.clone(), PendingOp::Refresh);
                effects.push(ClientEffect::Send(Command::StateSnapshot {
                    command_id: id,
                    session_id: sid,
                }));
            }
        }
        ClientIntent::SelectAgent { agent_instance_id } => {
            if let Some(session) = &state.live_session {
                let sid = session.session_id.clone();
                let id = ctx.command_ids.next_command_id();
                state.pending_commands.insert(
                    id.clone(),
                    PendingOp::SelectAgent {
                        agent_instance_id: agent_instance_id.clone(),
                    },
                );
                effects.push(ClientEffect::Send(Command::AgentSubscribe {
                    command_id: id,
                    session_id: sid,
                    agent_instance_id,
                    after_seq: None,
                }));
            }
        }
        ClientIntent::SubmitTurn { text } => {
            let text_trimmed = text.trim().to_string();
            if text_trimmed.is_empty() {
                return;
            }
            if let (SessionPhase::Live, Some(session)) = (&state.session_phase, &state.live_session)
                && let Some(agent_id) = &session.selected_agent
            {
                let sid = session.session_id.clone();
                let aid = agent_id.clone();
                let id = ctx.command_ids.next_command_id();
                state.pending_commands.insert(id.clone(), PendingOp::Submit);
                effects.push(ClientEffect::Send(Command::AgentInputSubmit {
                    input: user_agent_input(
                        &id,
                        sid,
                        aid,
                        piko_protocol::MessageContent::String(text_trimmed),
                    ),
                    command_id: id,
                }));
            }
        }
        ClientIntent::SubmitTurnMessage { content } => {
            if let (SessionPhase::Live, Some(session)) = (&state.session_phase, &state.live_session)
                && let Some(agent_id) = &session.selected_agent
            {
                let sid = session.session_id.clone();
                let aid = agent_id.clone();
                let id = ctx.command_ids.next_command_id();
                state.pending_commands.insert(id.clone(), PendingOp::Submit);
                effects.push(ClientEffect::Send(Command::AgentInputSubmit {
                    input: user_agent_input(&id, sid, aid, content),
                    command_id: id,
                }));
            }
        }
        ClientIntent::CancelTurn => {
            if let Some(session) = &state.live_session
                && let Some(agent_id) = &session.selected_agent
                && session
                    .agent_work
                    .get(agent_id)
                    .is_some_and(|work| work.active_work.is_some())
            {
                let sid = session.session_id.clone();
                let id = ctx.command_ids.next_command_id();
                state.pending_commands.insert(id.clone(), PendingOp::Cancel);
                effects.push(ClientEffect::Send(Command::AgentInterrupt {
                    command_id: id,
                    session_id: sid,
                    agent_instance_id: agent_id.clone(),
                }));
            }
        }
        ClientIntent::RespondApproval {
            approval_id,
            decision,
            note,
        } => {
            if let Some(session) = &mut state.live_session {
                let sid = session.session_id.clone();
                if let Some(ap) = session
                    .pending_approvals
                    .iter_mut()
                    .find(|a| a.approval_id == approval_id && !a.response_in_flight)
                {
                    ap.response_in_flight = true;
                    let id = ctx.command_ids.next_command_id();
                    state.pending_commands.insert(
                        id.clone(),
                        PendingOp::ApprovalRespond {
                            approval_id: approval_id.clone(),
                        },
                    );
                    effects.push(ClientEffect::Send(Command::ApprovalRespond {
                        command_id: id,
                        session_id: sid,
                        approval_id,
                        decision,
                        note,
                    }));
                }
            }
        }
        ClientIntent::RespondInteraction {
            interaction_id,
            response,
        } => {
            if let Some(session) = &mut state.live_session {
                let sid = session.session_id.clone();
                if let Some(ix) = session
                    .pending_interactions
                    .iter_mut()
                    .find(|i| i.interaction_id == interaction_id && !i.response_in_flight)
                {
                    ix.response_in_flight = true;
                    let id = ctx.command_ids.next_command_id();
                    state.pending_commands.insert(
                        id.clone(),
                        PendingOp::InteractionRespond {
                            interaction_id: interaction_id.clone(),
                        },
                    );
                    effects.push(ClientEffect::Send(Command::UserInteractionRespond {
                        command_id: id,
                        session_id: sid,
                        interaction_id,
                        response,
                    }));
                }
            }
        }
        ClientIntent::DeleteSession { session_id } => {
            let id = ctx.command_ids.next_command_id();
            state.pending_commands.insert(
                id.clone(),
                PendingOp::Delete {
                    session_id: session_id.clone(),
                },
            );
            effects.push(ClientEffect::Send(Command::SessionDelete {
                command_id: id,
                session_id,
            }));
        }
        ClientIntent::RenameSession { session_id, name } => {
            let id = ctx.command_ids.next_command_id();
            state.pending_commands.insert(
                id.clone(),
                PendingOp::Rename {
                    session_id: session_id.clone(),
                    name: name.clone(),
                },
            );
            effects.push(ClientEffect::Send(Command::SessionRename {
                command_id: id,
                session_id,
                name,
            }));
        }
        ClientIntent::NavigateSession {
            entry_id,
            summarize,
            custom_instructions,
        } => {
            if let Some(session) = &state.live_session {
                let sid = session.session_id.clone();
                let id = ctx.command_ids.next_command_id();
                state.pending_commands.insert(
                    id.clone(),
                    PendingOp::Navigate {
                        session_id: sid.clone(),
                    },
                );
                effects.push(ClientEffect::Send(Command::SessionNavigate {
                    command_id: id,
                    session_id: sid,
                    entry_id,
                    summarize,
                    custom_instructions,
                }));
            }
        }
        ClientIntent::ListModels => {
            let id = ctx.command_ids.next_command_id();
            state
                .pending_commands
                .insert(id.clone(), PendingOp::ListModels);
            effects.push(ClientEffect::Send(Command::ModelList { command_id: id }));
        }
        ClientIntent::SyncModelConfig => {
            // Empty merge patch: ModelRunnerObserver still emits ConfigChanged
            // with the current host defaults (same bootstrap trick as TUI).
            let id = ctx.command_ids.next_command_id();
            effects.push(ClientEffect::Send(Command::ConfigUpdate {
                command_id: id,
                patch: serde_json::json!({}),
            }));
        }
        ClientIntent::SetModel { provider, model_id } => {
            // ConfigUpdate has no typed CommandResponse; ModelEvent correlates.
            let id = ctx.command_ids.next_command_id();
            state.pending_commands.insert(
                id.clone(),
                PendingOp::SetModel {
                    provider: provider.clone(),
                    model_id: model_id.clone(),
                },
            );
            effects.push(ClientEffect::Send(Command::ConfigUpdate {
                command_id: id,
                patch: serde_json::json!({
                    "default-provider": provider,
                    "default-model": model_id,
                }),
            }));
        }
        ClientIntent::SetThinkingLevel { level } => {
            let id = ctx.command_ids.next_command_id();
            state.pending_commands.insert(
                id.clone(),
                PendingOp::SetThinkingLevel {
                    level: level.as_str().to_string(),
                },
            );
            effects.push(ClientEffect::Send(Command::ConfigUpdate {
                command_id: id,
                patch: serde_json::json!({
                    "default-thinking-level": level.as_str(),
                }),
            }));
        }
    }
}

fn user_agent_input(
    request_id: &str,
    session_id: String,
    agent_instance_id: String,
    content: piko_protocol::MessageContent,
) -> piko_protocol::AgentInput {
    let submitted_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default();
    piko_protocol::AgentInput {
        input_id: format!("input-{request_id}"),
        request_id: request_id.to_string(),
        session_id,
        agent_instance_id,
        origin: piko_protocol::AgentInputOrigin::User,
        delivery: piko_protocol::AgentInputDelivery::FollowUp,
        content,
        submitted_at,
        caller_agent_instance_id: None,
        detached_recipient_agent_instance_id: None,
    }
}

fn handle_transport(state: &mut ClientState, obs: TransportObservation) {
    match obs {
        TransportObservation::Connected => {
            state.shell.connection = ConnectionState::Connected;
        }
        TransportObservation::DecodeFailure { detail } => {
            state.last_error = Some(detail);
        }
        TransportObservation::SendFailure { detail } => {
            record_transport_failures(state, &detail);
            state.last_error = Some(detail);
            state.shell.connection = ConnectionState::Disconnected;
            if !state.is_live() {
                state.session_phase = if state.live_session.is_some() {
                    SessionPhase::Live
                } else {
                    SessionPhase::IdleNoSession
                };
            }
        }
        TransportObservation::Closed => {
            record_transport_failures(state, "host transport closed");
            state.shell.connection = ConnectionState::Disconnected;
        }
    }
}

fn record_transport_failures(state: &mut ClientState, detail: &str) {
    for (command_id, operation) in state.pending_commands.drain() {
        state.command_failures.push(crate::state::CommandFailure {
            command_id,
            operation,
            message: detail.to_string(),
        });
    }
    const MAX_COMMAND_FAILURES: usize = 50;
    if state.command_failures.len() > MAX_COMMAND_FAILURES {
        state
            .command_failures
            .drain(..state.command_failures.len() - MAX_COMMAND_FAILURES);
    }
}
