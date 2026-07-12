use std::sync::Arc;

use piko_protocol::agent_runtime::{SessionEvent, SessionOutput};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_stream::StreamExt;

use crate::adapters::prompts::agent_loader::load_agents;
use crate::adapters::turns::session_output::{
    is_execution_terminal, realtime_message_from_delta, record_committed_message,
};
use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::infra::storage::SessionStore;
use crate::ports::TurnRunner;
use crate::util::{now_ms, send_event};

impl HostApp {
    /// Drive one Turn's session output stream to completion: apply realtime
    /// deltas and execution/message-committed events, reconnecting on stream
    /// exhaustion, until the root execution reaches a terminal status. Returns
    /// whether the turn completed successfully (used by the caller to decide
    /// whether to run compaction / drain the follow-up queue).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_turn_observation_loop(
        &self,
        runner: &Arc<dyn TurnRunner>,
        session_id: &str,
        turn_id: &str,
        cwd: &str,
        session_dir: &std::path::Path,
        mut output: orchd_api::SessionOutputStream,
        mut ui_event_rx: UnboundedReceiver<ServerMessage>,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<bool, ProtocolError> {
        let mut total_tasks: u32 = 0;
        let mut turn_done = false;
        let mut root_terminal_status: Option<piko_protocol::ExecutionStatus> = None;
        let agent_specs = load_agents(cwd);

        while !turn_done {
            tokio::select! {
                ui_event = ui_event_rx.recv() => {
                    if let Some(event) = ui_event {
                        send_event(tx, event);
                    }
                }
                item = output.next() => {
                    let Some(item) = item else {
                        tracing::warn!(session_id, "session output closed; reconnecting");
                        let (runtime_snapshot, recovered) =
                            runner.recover_session_subscription(session_id).await?;
                        let (snapshot, agents) = {
                            let state = self.state.lock().await;
                            (
                                state.snapshot(session_id)?,
                                state.get_agent_list(session_id),
                            )
                        };
                        send_event(
                            tx,
                            ServerMessage::SessionReconciled(piko_protocol::SessionReconciledEvent {
                                session_id: session_id.to_string(),
                                reason: piko_protocol::ReconcileReason::Reconnect,
                                cursor: runtime_snapshot.cursor,
                                snapshot,
                                agents,
                            }),
                        );
                        output = recovered.output;
                        continue;
                    };
                    let envelope = match item {
                        Ok(envelope) => envelope,
                        Err(orchd_api::SessionStreamError::SnapshotRequired { reason }) => {
                            tracing::warn!(session_id, ?reason, "reconciling exhausted session output");
                            let (runtime_snapshot, recovered) =
                                runner.recover_session_subscription(session_id).await?;
                            let (snapshot, agents) = {
                                let state = self.state.lock().await;
                                (
                                    state.snapshot(session_id)?,
                                    state.get_agent_list(session_id),
                                )
                            };
                            send_event(
                                tx,
                                ServerMessage::SessionReconciled(piko_protocol::SessionReconciledEvent {
                                    session_id: session_id.to_string(),
                                    reason: piko_protocol::ReconcileReason::RetentionExhausted,
                                    cursor: runtime_snapshot.cursor,
                                    snapshot,
                                    agents,
                                }),
                            );
                            output = recovered.output;
                            continue;
                        }
                        Err(error) => {
                            return Err(ProtocolError::ObservationFailed(format!(
                                "session {session_id}: {error}"
                            )));
                        }
                    };

                    match envelope.output {
                        SessionOutput::Delta(delta_envelope) => {
                            let Some(event) = realtime_message_from_delta(session_id, &delta_envelope)
                            else {
                                tracing::warn!(
                                    session_id,
                                    execution_id = %delta_envelope.execution_id,
                                    agent_id = %delta_envelope.agent_id,
                                    delta_seq = delta_envelope.delta_seq,
                                    "dropping realtime delta without message identity"
                                );
                                continue;
                            };
                            send_event(tx, ServerMessage::RealtimeMessage(event));
                        }
                        SessionOutput::Event(event_envelope) => match &event_envelope.event {
                            SessionEvent::ExecutionChanged { snapshot } => {
                                tracing::info!(
                                    session_id = %session_id,
                                    turn_id = %turn_id,
                                    execution_id = %snapshot.execution_id,
                                    status = ?snapshot.status,
                                    "observed execution changed"
                                );

                                self.apply_execution_observation(
                                    runner,
                                    cwd,
                                    session_id,
                                    &agent_specs,
                                    snapshot,
                                    &mut total_tasks,
                                    tx,
                                )
                                .await?;

                                if is_execution_terminal(&snapshot.status) {
                                    tracing::info!(
                                        session_id = %session_id,
                                        turn_id = %turn_id,
                                        execution_id = %snapshot.execution_id,
                                        status = ?snapshot.status,
                                        "root execution reached terminal status; ending turn"
                                    );
                                    root_terminal_status = Some(snapshot.status.clone());
                                    turn_done = true;
                                }
                            }
                            SessionEvent::MessageCommitted {
                                message_id,
                                work_id: _,
                                role,
                            } => {
                                tracing::info!(
                                    session_id = %session_id,
                                    turn_id = %turn_id,
                                    agent_instance_id = %event_envelope.agent_instance_id,
                                    message_id = %message_id,
                                    ?role,
                                    "observed message committed"
                                );
                                let committed = {
                                    let mut state = self.state.lock().await;
                                    let store = SessionStore::new(session_dir);
                                    record_committed_message(
                                        &mut state,
                                        Some(&store),
                                        session_id,
                                        &event_envelope.agent_instance_id,
                                        message_id,
                                    )?
                                };
                                if let Some(committed) = committed {
                                    send_event(tx, ServerMessage::TranscriptCommitted(committed));
                                } else {
                                    tracing::error!(
                                        session_id = %session_id,
                                        turn_id = %turn_id,
                                        agent_instance_id = %event_envelope.agent_instance_id,
                                        message_id = %message_id,
                                        "committed projection missing; aborting turn observation"
                                    );
                                    return Err(ProtocolError::ObservationFailed(format!(
                                        "committed projection {message_id} missing for agent {}",
                                        event_envelope.agent_instance_id
                                    )));
                                }
                            }
                            SessionEvent::ToolCommitted {
                                message_id,
                                work_id: _,
                                ..
                            } => {
                                let committed = {
                                    let mut state = self.state.lock().await;
                                    let store = SessionStore::new(session_dir);
                                    record_committed_message(
                                        &mut state,
                                        Some(&store),
                                        session_id,
                                        &event_envelope.agent_instance_id,
                                        message_id,
                                    )?
                                };
                                if let Some(committed) = committed {
                                    send_event(tx, ServerMessage::TranscriptCommitted(committed));
                                } else {
                                    return Err(ProtocolError::ObservationFailed(format!(
                                        "committed tool projection {message_id} missing for agent {}",
                                        event_envelope.agent_instance_id
                                    )));
                                }
                            }
                            _ => {}
                        },
                    }
                }
            }
        }

        let complete_event = {
            let mut state = self.state.lock().await;
            let still_active = state
                .session(session_id)
                .ok()
                .and_then(|s| s.active_turn_id.clone())
                .as_deref()
                == Some(turn_id);
            if !still_active {
                // Already finalized (e.g. TurnCancel cleared active_turn_id).
                None
            } else {
                match root_terminal_status {
                    Some(piko_protocol::ExecutionStatus::Failed) => {
                        Some(state.fail_turn(session_id, turn_id, "execution failed")?)
                    }
                    Some(piko_protocol::ExecutionStatus::Cancelled) => {
                        Some(state.cancel_turn(session_id, turn_id)?)
                    }
                    _ => {
                        state.clear_active_turn(session_id, turn_id)?;
                        Some(ServerMessage::TurnLifecycle(
                            crate::api::TurnEvent::Completed {
                                session_id: session_id.to_string(),
                                turn_id: turn_id.to_string(),
                                total_tasks: total_tasks.max(1),
                                timestamp: now_ms(),
                            },
                        ))
                    }
                }
            }
        };
        let turn_succeeded = matches!(
            (&complete_event, &root_terminal_status),
            (
                Some(ServerMessage::TurnLifecycle(
                    crate::api::TurnEvent::Completed { .. }
                )),
                None | Some(piko_protocol::ExecutionStatus::Succeeded)
            )
        );
        if let Some(complete_event) = complete_event {
            tracing::info!(
                session_id = %session_id,
                turn_id = %turn_id,
                total_tasks,
                "turn observation loop finished; emitting terminal"
            );
            send_event(tx, complete_event);
        } else {
            tracing::info!(
                session_id = %session_id,
                turn_id = %turn_id,
                "turn observation loop finished; turn already terminal"
            );
        }

        Ok(turn_succeeded)
    }
}
