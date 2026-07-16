use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::ports::{AgentOperationAddress, AgentRunHandle, AgentRunRunner};
use crate::util::send_event;

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
        ui_event_rx: UnboundedReceiver<ServerMessage>,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<bool, ProtocolError> {
        let address = AgentOperationAddress {
            session_id: session_id.to_string(),
            operation_id: turn_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
        };
        let AgentRunHandle {
            started,
            completion,
            ..
        } = run;
        let observation = started.await.map_err(|_| {
            ProtocolError::ObservationFailed("Agent run start signal closed".into())
        })?;
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
        );
        let completion = self
            .drive_operation_observation(
                runner,
                &address,
                session_dir,
                observation,
                completion,
                ui_event_rx,
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
            send_event(tx, complete_event);
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
}
