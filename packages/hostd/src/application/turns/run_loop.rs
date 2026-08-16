use std::sync::Arc;

use piko_protocol::{
    TrajectoryIdentity, TrajectoryNotificationKind, TrajectoryRecord,
    TrajectorySystemNotificationRecord, TrajectoryTerminalKind, TrajectoryTerminalRecord,
};

use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::ports::{AgentOperationAddress, AgentRunFailure, AgentRunHandle, AgentRunRunner};
use crate::util::{ClientEventSender, send_event};

impl HostApp {
    /// Catalog-resolved context window for client chrome (no compaction fallback).
    pub(crate) async fn client_context_window_size(&self) -> Option<u64> {
        let (model, provider) = {
            let settings = self.settings.lock().await;
            (
                settings.default_model.clone(),
                settings.default_provider.clone(),
            )
        };
        let model_id = model.filter(|id| !id.is_empty())?;
        self.model_registry
            .lock()
            .await
            .resolve(Some(model_id.as_str()), provider.as_deref())
            .map(|resolved| resolved.model.context_window)
            .filter(|window| *window > 0)
    }

    /// Emit a terminal turn event followed by a host usage projection when applicable.
    pub(crate) async fn send_turn_terminal(&self, tx: &ClientEventSender, terminal: ServerMessage) {
        let size = self.client_context_window_size().await;
        let session_id = match &terminal {
            ServerMessage::TurnLifecycle(
                crate::api::TurnEvent::Completed { session_id, .. }
                | crate::api::TurnEvent::Failed { session_id, .. }
                | crate::api::TurnEvent::Cancelled { session_id, .. },
            ) => Some(session_id.clone()),
            _ => None,
        };
        let messages = {
            let state = self.state.lock().await;
            let mut messages = state.with_usage_projection(terminal, size);
            if let Some(session_id) = session_id {
                messages.push(state.build_queue_update(&session_id).into());
            }
            messages
        };
        for message in messages {
            send_event(tx, message).await;
        }
    }
}

impl HostApp {
    /// Drive one Turn's session output stream to completion: apply realtime
    /// deltas and committed-message events, reconnecting on stream exhaustion,
    /// until the durable Agent run result reaches its observation barrier. Returns
    /// whether the turn completed successfully (used by the caller to decide
    /// whether to run compaction / drain the follow-up queue).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_turn_observation_loop(
        &self,
        runner: &Arc<dyn AgentRunRunner>,
        session_id: &str,
        turn_id: &str,
        agent_instance_id: &str,
        session_dir: &std::path::Path,
        run: AgentRunHandle,
        tx: &ClientEventSender,
    ) -> Result<bool, ProtocolError> {
        let address = AgentOperationAddress {
            session_id: session_id.to_string(),
            operation_id: turn_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
        };
        let AgentRunHandle { mut process, .. } = run;
        let observation = process.wait_started().await?;
        self.state
            .lock()
            .await
            .mark_turn_running(session_id, turn_id)?;
        send_event(
            tx,
            ServerMessage::TurnLifecycle(crate::api::TurnEvent::Started {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                timestamp: crate::util::now_ms(),
            }),
        )
        .await;
        let completion = self
            .drive_operation_observation(
                runner,
                &address,
                session_dir,
                observation,
                process.wait_completion(),
                tx,
            )
            .await?;
        if let Ok(report) = &completion.result
            && report.agent_instance_id != agent_instance_id
        {
            return Err(ProtocolError::ObservationFailed(format!(
                "Agent report identity mismatch: expected {}, got {}",
                agent_instance_id, report.agent_instance_id
            )));
        }
        let barrier = completion.observation_barrier.clone();
        let terminal = completion.result;
        // F-36: record the terminal outcome on the durable trajectory. The
        // terminal record is appended after the `execution_finished` fact, so
        // its SSE fan-out makes a live viewer observe the running → terminal
        // transition (on a clean completion no other trajectory record would
        // follow the fact). Failures additionally keep the RunError
        // notification so the human-readable reason is visible in the stream.
        self.record_trajectory_terminal(&address, &terminal).await;
        // F-36: record run failures on the durable trajectory.
        if let Err(failure) = &terminal {
            self.record_trajectory_run_error(&address, failure.message.clone())
                .await;
        } else if let Ok(report) = &terminal
            && matches!(
                report.outcome,
                piko_protocol::ExecutionOutcome::Failed { .. }
            )
        {
            self.record_trajectory_run_error(&address, "agent run failed".into())
                .await;
        }

        let complete_event = {
            let mut state = self.state.lock().await;
            let still_active = state.turn(session_id, turn_id).is_ok_and(|turn| {
                matches!(
                    turn.status,
                    crate::api::TurnStatus::Running | crate::api::TurnStatus::Cancelling
                )
            });
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
                "turn observation loop finished; emitting terminal"
            );
            self.send_turn_terminal(tx, complete_event).await;
        } else {
            tracing::info!(
                session_id = %session_id,
                turn_id = %turn_id,
                "turn observation loop finished; turn already terminal"
            );
        }

        runner.finish_agent_run(&address, &barrier).await;

        Ok(turn_succeeded)
    }

    /// Trajectory run identity for terminal/error records: the orchd
    /// execution id, derived as `stable("exec", [session, agent, request_id])`
    /// with the root-turn request id equal to the hostd operation id.
    fn trajectory_identity(&self, address: &AgentOperationAddress) -> TrajectoryIdentity {
        TrajectoryIdentity {
            session_id: address.session_id.clone(),
            agent_instance_id: address.agent_instance_id.clone(),
            run_id: piko_orchd_api::stable_internal_id(
                "exec",
                &[
                    &address.session_id,
                    &address.agent_instance_id,
                    &address.operation_id,
                ],
            ),
            execution_id: None,
            source_turn_id: Some(address.operation_id.clone()),
        }
    }

    /// Record the run's terminal outcome as the final trajectory record.
    async fn record_trajectory_terminal(
        &self,
        address: &AgentOperationAddress,
        terminal: &Result<piko_protocol::AgentRunReport, AgentRunFailure>,
    ) {
        let runner = self.turn_runner.lock().await.clone();
        let Some(recorder) = runner.trajectory_registry().get(&address.session_id) else {
            return;
        };
        let (kind, reason) = match terminal {
            Ok(report) => match &report.outcome {
                piko_protocol::ExecutionOutcome::Succeeded { .. } => {
                    (TrajectoryTerminalKind::Completed, None)
                }
                piko_protocol::ExecutionOutcome::Failed { error } => {
                    (TrajectoryTerminalKind::Failed, Some(error.clone()))
                }
                piko_protocol::ExecutionOutcome::Cancelled { reason } => {
                    (TrajectoryTerminalKind::Cancelled, reason.clone())
                }
            },
            Err(failure) => (
                TrajectoryTerminalKind::Failed,
                Some(failure.message.clone()),
            ),
        };
        recorder
            .record(TrajectoryRecord::Terminal(TrajectoryTerminalRecord {
                identity: self.trajectory_identity(address),
                kind,
                reason,
                finished_at: crate::util::now_ms(),
            }))
            .await;
    }

    async fn record_trajectory_run_error(&self, address: &AgentOperationAddress, message: String) {
        let runner = self.turn_runner.lock().await.clone();
        let Some(recorder) = runner.trajectory_registry().get(&address.session_id) else {
            return;
        };
        recorder
            .record(TrajectoryRecord::SystemNotification(
                TrajectorySystemNotificationRecord {
                    identity: self.trajectory_identity(address),
                    kind: TrajectoryNotificationKind::RunError,
                    summary: message,
                    recorded_at: crate::util::now_ms(),
                },
            ))
            .await;
    }
}
