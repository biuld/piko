use std::sync::Arc;

use piko_protocol::agent_runtime::{SessionEvent, SessionOutput};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_stream::StreamExt;

use crate::adapters::turns::session_output::{
    realtime_message_from_delta, reconcile_committed_messages, record_committed_message,
};
use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::infra::storage::SessionStore;
use crate::ports::{TurnRunHandle, TurnRunner};
use crate::util::send_event;

impl HostApp {
    /// Drive one Turn's session output stream to completion: apply realtime
    /// deltas and committed-message events, reconnecting on stream exhaustion,
    /// until the durable root Agent run result reaches its observation barrier. Returns
    /// whether the turn completed successfully (used by the caller to decide
    /// whether to run compaction / drain the follow-up queue).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_turn_observation_loop(
        &self,
        runner: &Arc<dyn TurnRunner>,
        session_id: &str,
        turn_id: &str,
        session_dir: &std::path::Path,
        turn_run: TurnRunHandle,
        mut ui_event_rx: UnboundedReceiver<ServerMessage>,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<bool, ProtocolError> {
        let total_tasks: u32 = 1;
        let mut output = turn_run.observation.output;
        let mut observed_cursor = turn_run.observation.cursor;
        let mut completion_rx = turn_run.completion;
        let mut completion: Option<crate::ports::TurnRunCompletion> = None;
        let mut ui_events_open = true;

        loop {
            if completion.as_ref().is_some_and(|completion| {
                cursor_reached(&observed_cursor, &completion.observation_barrier)
            }) {
                break;
            }
            tokio::select! {
                biased;
                result = &mut completion_rx, if completion.is_none() => {
                    let completed = result.map_err(|_| {
                        ProtocolError::ObservationFailed(format!(
                            "Turn run completion channel closed for {session_id}/{turn_id}"
                        ))
                    })?;
                    if completed.session_id != session_id || completed.turn_id != turn_id {
                        return Err(ProtocolError::ObservationFailed(format!(
                            "Turn run completion identity mismatch: expected {session_id}/{turn_id}, got {}/{}",
                            completed.session_id, completed.turn_id
                        )));
                    }
                    if let Ok(report) = &completed.result
                        && report.agent_instance_id != completed.root_agent_instance_id
                    {
                        return Err(ProtocolError::ObservationFailed(format!(
                            "root Agent report identity mismatch: expected {}, got {}",
                            completed.root_agent_instance_id, report.agent_instance_id
                        )));
                    }
                    completion = Some(completed);
                }
                ui_event = ui_event_rx.recv(), if ui_events_open => {
                    if let Some(event) = ui_event {
                        send_event(tx, event);
                    } else {
                        ui_events_open = false;
                    }
                }
                item = output.next() => {
                    let Some(item) = item else {
                        tracing::warn!(session_id, "session output closed; reconnecting");
                        let (runtime_snapshot, recovered) =
                            runner.recover_observation(session_id).await?;
                        let (snapshot, agents) = {
                            let mut state = self.state.lock().await;
                            let store = SessionStore::new(session_dir);
                            reconcile_committed_messages(&mut state, &store, session_id)?;
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
                                cursor: runtime_snapshot.cursor.clone(),
                                snapshot,
                                agents,
                            }),
                        );
                        observed_cursor = runtime_snapshot.cursor;
                        output = recovered.output;
                        continue;
                    };
                    let envelope = match item {
                        Ok(envelope) => envelope,
                        Err(orchd_api::SessionStreamError::SnapshotRequired { reason }) => {
                            tracing::warn!(session_id, ?reason, "reconciling exhausted session output");
                            let (runtime_snapshot, recovered) =
                                runner.recover_observation(session_id).await?;
                            let (snapshot, agents) = {
                                let mut state = self.state.lock().await;
                                let store = SessionStore::new(session_dir);
                                reconcile_committed_messages(&mut state, &store, session_id)?;
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
                                    cursor: runtime_snapshot.cursor.clone(),
                                    snapshot,
                                    agents,
                                }),
                            );
                            observed_cursor = runtime_snapshot.cursor;
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
                        SessionOutput::Event(event_envelope) => {
                            observed_cursor = event_envelope.cursor.clone();
                            match &event_envelope.event {
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
                        }
                        },
                    }
                }
            }
        }

        let completion = completion.expect("completion checked before leaving observation loop");
        let barrier = completion.observation_barrier.clone();
        let terminal = completion.result;

        let complete_event = {
            let mut state = self.state.lock().await;
            let still_active = state
                .session(session_id)
                .ok()
                .and_then(|s| s.active_turn_id.clone())
                .as_deref()
                == Some(turn_id);
            if !still_active {
                // A replayed/recovered completion may find an already-terminal Turn.
                None
            } else {
                match &terminal {
                    Ok(report)
                        if matches!(
                            report.outcome,
                            piko_protocol::ExecutionOutcome::Failed { .. }
                        ) =>
                    {
                        Some(state.fail_turn(session_id, turn_id, "agent run failed")?)
                    }
                    Ok(report)
                        if matches!(
                            report.outcome,
                            piko_protocol::ExecutionOutcome::Cancelled { .. }
                        ) =>
                    {
                        Some(state.cancel_turn(session_id, turn_id)?)
                    }
                    Err(failure) => {
                        Some(state.fail_turn(session_id, turn_id, failure.message.clone())?)
                    }
                    _ => Some(state.complete_turn(session_id, turn_id)?),
                }
            }
        };
        let turn_succeeded = matches!(
            (&complete_event, &terminal),
            (
                Some(ServerMessage::TurnLifecycle(
                    crate::api::TurnEvent::Completed { .. }
                )),
                Ok(_)
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

        runner
            .acknowledge_turn_run(session_id, turn_id, &barrier)
            .await;

        Ok(turn_succeeded)
    }
}

fn cursor_reached(
    observed: &piko_protocol::agent_runtime::SessionCursor,
    barrier: &piko_protocol::agent_runtime::SessionCursor,
) -> bool {
    observed.epoch == barrier.epoch && observed.seq >= barrier.seq
}
