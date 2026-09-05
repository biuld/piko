use std::{sync::Arc, time::Instant};

use piko_protocol::{
    TrajectoryIdentity, TrajectoryNotificationKind, TrajectoryRecord,
    TrajectorySystemNotificationRecord, TrajectoryTerminalKind, TrajectoryTerminalRecord,
};

use crate::api::ProtocolError;
use crate::application::host_app::HostApp;
use crate::ports::{AgentRunFailure, AgentRunRunner};
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

    /// Drive one admitted root AgentInput's session output stream to
    /// completion: apply realtime deltas and committed-message events,
    /// reconnecting on stream exhaustion, until the durable Agent work result
    /// reaches its observation barrier. Returns whether the work completed
    /// successfully (used by the caller to decide whether to run compaction /
    /// drain the follow-up queue).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_turn_observation_loop(
        &self,
        runner: &Arc<dyn AgentRunRunner>,
        session_id: &str,
        input_id: &str,
        agent_instance_id: &str,
        session_dir: &std::path::Path,
        receipt: &piko_protocol::AgentInputReceipt,
        tx: &ClientEventSender,
    ) -> Result<bool, ProtocolError> {
        let started_at = Instant::now();
        let observation = match runner
            .wait_agent_input_started(session_id, agent_instance_id, input_id, receipt.disposition)
            .await
        {
            Ok(observation) => observation,
            Err(error) if error.to_string().to_ascii_lowercase().contains("cancel") => {
                runner
                    .finish_agent_run(session_id, agent_instance_id, input_id)
                    .await;
                self.publish_work_reconcile(session_id, tx).await?;
                return Ok(false);
            }
            Err(error) => return Err(error),
        };
        let completion_runner = Arc::clone(runner);
        let completion_sid = session_id.to_string();
        let completion_aid = agent_instance_id.to_string();
        let completion_iid = input_id.to_string();
        let completion_future = Box::pin(async move {
            completion_runner
                .wait_agent_input_completion(&completion_sid, &completion_aid, &completion_iid)
                .await
        });
        let completion = self
            .drive_operation_observation(
                runner,
                session_id,
                agent_instance_id,
                input_id,
                session_dir,
                observation,
                completion_future,
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
        let terminal = completion.result;
        // F-36: record the terminal outcome on the durable trajectory. The
        // terminal record is appended after the processing-finished fact so
        // history diagnostics can show the running → terminal transition (on
        // a clean completion no other trajectory record would follow the
        // fact). Failures additionally keep the RunError notification so the
        // human-readable reason is visible in the diagnostic stream.
        self.record_trajectory_terminal(session_id, agent_instance_id, input_id, &terminal)
            .await;
        // F-36: record run failures on the durable trajectory.
        if let Err(failure) = &terminal {
            self.record_trajectory_run_error(
                session_id,
                agent_instance_id,
                input_id,
                failure.message.clone(),
            )
            .await;
        } else if let Ok(report) = &terminal
            && matches!(
                report.outcome,
                piko_protocol::AgentWorkOutcome::Failed { .. }
            )
        {
            self.record_trajectory_run_error(
                session_id,
                agent_instance_id,
                input_id,
                "agent run failed".into(),
            )
            .await;
        }

        let turn_succeeded = matches!(
            &terminal,
            Ok(report) if matches!(report.outcome, piko_protocol::AgentWorkOutcome::Succeeded { .. })
        );
        if let Ok(report) = &terminal {
            let status = match report.outcome {
                piko_protocol::AgentWorkOutcome::Succeeded { .. } => "completed",
                piko_protocol::AgentWorkOutcome::Failed { .. } => "failed",
                piko_protocol::AgentWorkOutcome::Cancelled { .. } => "cancelled",
            };
            crate::telemetry::handle().record_turn(
                started_at.elapsed().as_millis() as u64,
                status,
                "agent_input",
            );
            crate::telemetry::handle().record_input_usage(&report.usage, status);
            let size = self.client_context_window_size().await;
            if let Ok(usage) = self.state.lock().await.usage_updated_event(
                session_id,
                Some(agent_instance_id.to_string()),
                Some(input_id.to_string()),
                Some(&report.usage),
                size,
            ) {
                send_event(tx, usage).await;
            }
        }

        runner
            .finish_agent_run(session_id, agent_instance_id, input_id)
            .await;

        Ok(turn_succeeded)
    }

    pub(super) async fn publish_work_reconcile(
        &self,
        session_id: &str,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        if let Some(session_dir) = self.session_paths.lock().await.get(session_id).cloned() {
            let store = self.session_store_factory.open(&session_dir);
            let mut state = self.state.lock().await;
            crate::application::agent_work::projection::reconcile_committed_messages(
                &mut state,
                store.as_ref(),
                session_id,
            )
            .await?;
        }
        let (snapshot, agents) = self.session_view(session_id).await?;
        send_event(
            tx,
            crate::application::sessions::helpers::session_reconciled_message(
                session_id.to_string(),
                piko_protocol::ReconcileReason::ExplicitRefresh,
                snapshot,
                agents,
            ),
        )
        .await;
        Ok(())
    }

    /// Trajectory identity for terminal/error records. The work identity is the
    /// root input id; the concrete runtime instance is derived deterministically.
    fn trajectory_identity(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> TrajectoryIdentity {
        TrajectoryIdentity {
            session_id: session_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            root_input_id: input_id.to_string(),
        }
    }

    /// Record the work's terminal outcome as the final trajectory record.
    async fn record_trajectory_terminal(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        terminal: &Result<piko_protocol::AgentWorkReport, AgentRunFailure>,
    ) {
        let runner = self.agent_runner.lock().await.clone();
        let Some(recorder) = runner.trajectory_registry().get(session_id) else {
            return;
        };
        let (kind, reason) = match terminal {
            Ok(report) => match &report.outcome {
                piko_protocol::AgentWorkOutcome::Succeeded { .. } => {
                    (TrajectoryTerminalKind::Completed, None)
                }
                piko_protocol::AgentWorkOutcome::Failed { error } => {
                    (TrajectoryTerminalKind::Failed, Some(error.clone()))
                }
                piko_protocol::AgentWorkOutcome::Cancelled { reason } => {
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
                identity: self.trajectory_identity(session_id, agent_instance_id, input_id),
                kind,
                reason,
                finished_at: crate::util::now_ms(),
            }))
            .await;
    }

    async fn record_trajectory_run_error(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        message: String,
    ) {
        let runner = self.agent_runner.lock().await.clone();
        let Some(recorder) = runner.trajectory_registry().get(session_id) else {
            return;
        };
        recorder
            .record(TrajectoryRecord::SystemNotification(
                TrajectorySystemNotificationRecord {
                    identity: self.trajectory_identity(session_id, agent_instance_id, input_id),
                    kind: TrajectoryNotificationKind::RunError,
                    summary: message,
                    recorded_at: crate::util::now_ms(),
                },
            ))
            .await;
    }
}
