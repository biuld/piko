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
        let completion = self
            .drive_operation_observation(
                runner,
                &address,
                session_dir,
                run.observation,
                run.completion,
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
            let still_active = state
                .active_turn_for_agent(session_id, agent_instance_id)
                .is_some_and(|turn| turn.turn_id == turn_id);
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
