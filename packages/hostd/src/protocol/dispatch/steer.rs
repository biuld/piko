use piko_protocol::MessageContent;

use super::*;

impl HostServer {
    pub(super) async fn apply_steer_message(
        &self,
        command_id: String,
        session_id: String,
        agent_instance_id: String,
        content: MessageContent,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        crate::application::turns::content::validate_user_content(&content)?;
        let can_steer = {
            let state = self.state.lock().await;
            state
                .active_turn_for_agent(&session_id, &agent_instance_id)
                .is_some_and(|turn| {
                    matches!(
                        turn.status,
                        crate::api::TurnStatus::Running
                            | crate::api::TurnStatus::WaitingForApproval
                    )
                })
        };
        if !can_steer {
            return Err(ProtocolError::InvalidCommand(format!(
                "agent {agent_instance_id} is not running; cannot steer"
            )));
        }
        let runner = self.turn_runner.lock().await.clone();
        if !runner
            .steer_agent(&session_id, &agent_instance_id, content.clone())
            .await
        {
            return Err(ProtocolError::InvalidCommand(format!(
                "steer rejected for agent {agent_instance_id}"
            )));
        }
        let preview = crate::application::turns::content::text_projection(&content);
        let queue_event = {
            let mut state = self.state.lock().await;
            state.push_steer(&session_id, &agent_instance_id, &preview)
        };
        Ok(vec![
            ServerMessage::CommandResponse {
                command_id,
                result: Ok(crate::api::CommandResult::Empty),
            },
            queue_event.into(),
        ])
    }
}
