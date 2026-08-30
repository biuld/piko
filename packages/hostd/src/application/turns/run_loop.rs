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
        let messages = {
            let state = self.state.lock().await;
            state.with_usage_projection(terminal, size)
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
        let observation = match process.wait_started().await {
            Ok(observation) => observation,
            Err(error) if error.to_string().to_ascii_lowercase().contains("cancel") => {
                self.send_turn_terminal(
                    tx,
                    crate::domain::sessions::turn_cancelled(
                        session_id,
                        turn_id,
                        agent_instance_id,
                        piko_protocol::Usage::empty(),
                    ),
                )
                .await;
                runner
                    .finish_agent_run(
                        &address,
                        &piko_protocol::agent_runtime::SessionCursor {
                            epoch: String::new(),
                            seq: 0,
                        },
                    )
                    .await;
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        send_event(
            tx,
            crate::domain::sessions::turn_started(session_id, turn_id, agent_instance_id),
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

        let complete_event = match &terminal {
            Ok(report) => Some(crate::domain::sessions::turn_terminal_from_report(
                session_id, turn_id, report,
            )),
            Err(failure) => Some(crate::domain::sessions::turn_failed(
                session_id,
                turn_id,
                agent_instance_id,
                failure.message.clone(),
                piko_protocol::Usage::empty(),
            )),
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

    /// Trajectory identity for terminal/error records. The operation is the
    /// logical Run; its concrete Execution is the stable runtime instance
    /// derived from the session, agent, and operation.
    fn trajectory_identity(&self, address: &AgentOperationAddress) -> TrajectoryIdentity {
        TrajectoryIdentity {
            session_id: address.session_id.clone(),
            agent_instance_id: address.agent_instance_id.clone(),
            run_id: address.operation_id.clone(),
            execution_id: Some(piko_orchd_api::stable_internal_id(
                "exec",
                &[
                    &address.session_id,
                    &address.agent_instance_id,
                    &address.operation_id,
                ],
            )),
            source_turn_id: Some(address.operation_id.clone()),
        }
    }

    /// Record the run's terminal outcome as the final trajectory record.
    async fn record_trajectory_terminal(
        &self,
        address: &AgentOperationAddress,
        terminal: &Result<piko_protocol::AgentWorkReport, AgentRunFailure>,
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
