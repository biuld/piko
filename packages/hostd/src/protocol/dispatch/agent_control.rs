use super::*;

impl HostServer {
    pub(super) async fn apply_agent_interrupt(
        &self,
        command_id: String,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let prior_turn = {
            let state = self.state.lock().await;
            state.session(&session_id)?;
            state
                .active_turn_for_agent(&session_id, &agent_instance_id)
                .filter(|turn| {
                    matches!(
                        turn.status,
                        crate::api::TurnStatus::Running
                            | crate::api::TurnStatus::WaitingForApproval
                            | crate::api::TurnStatus::Cancelling
                    )
                })
                .map(|turn| (turn.turn_id.clone(), turn.status))
        };
        if let Some((turn_id, status)) = &prior_turn
            && *status != crate::api::TurnStatus::Cancelling
        {
            self.state.lock().await.set_turn_status(
                &session_id,
                turn_id,
                crate::api::TurnStatus::Cancelling,
            )?;
        }

        let runner = self.turn_runner.lock().await.clone();
        let accepted = runner
            .interrupt_agent(&session_id, &agent_instance_id)
            .await;
        if !accepted
            && let Some((turn_id, status)) = prior_turn
            && status != crate::api::TurnStatus::Cancelling
        {
            self.state
                .lock()
                .await
                .set_turn_status(&session_id, &turn_id, status)?;
        }

        Ok(vec![ServerMessage::CommandResponse {
            command_id,
            result: Ok(crate::api::CommandResult::AgentInterrupted {
                session_id,
                agent_instance_id,
                accepted,
                timestamp: now_ms(),
            }),
        }])
    }
}
