use super::*;

impl HostServer {
    pub(crate) async fn apply_command_stream(
        &self,
        command: Command,
        command_id: String,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        match command {
            Command::AuthLoginOAuth { provider, .. } => {
                self.start_oauth_login(&command_id, provider, tx);
                Ok(())
            }
            Command::ChatSubmit {
                session_id,
                target_agent_instance_id,
                text,
                ..
            } => {
                self.0
                    .apply_chat_submit(command_id, session_id, target_agent_instance_id, text, tx)
                    .await
            }
            Command::SessionCompact {
                session_id,
                agent_instance_id,
                mode,
                ..
            } => {
                // Manual compaction — bypass threshold, always compact.
                send_event(
                    tx,
                    ServerMessage::CommandResponse {
                        command_id: command_id.clone(),
                        result: Ok(crate::api::CommandResult::Empty),
                    },
                )
                .await;
                if let Err(error) = self
                    .0
                    .compact_session_if_needed(
                        &session_id,
                        &agent_instance_id,
                        0,
                        mode,
                        true,
                        Some(tx),
                    )
                    .await
                {
                    tracing::warn!(
                        session_id,
                        error = %error,
                        "session.compact failed"
                    );
                }
                Ok(())
            }
            command => {
                let events = self.apply_command(command).await?;
                for event in events {
                    send_event(tx, event).await;
                }
                Ok(())
            }
        }
    }
}
