use super::*;

impl HostServer {
    pub(crate) async fn apply_command_stream(
        &self,
        command: Command,
        command_id: String,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        match command {
            Command::AuthLoginOAuth { provider, mode, .. } => {
                self.start_oauth_login(&command_id, provider, mode, tx)
                    .await
            }
            Command::AgentInputSubmit { input, .. } => {
                crate::application::AgentWorkControl::new(&self.0)
                    .submit(command_id, input, tx)
                    .await
            }
            Command::SessionCompact {
                session_id,
                agent_instance_id,
                mode,
                ..
            } => {
                // Manual compaction — bypass threshold, always compact. The
                // command response is correlated and terminal: it reports the
                // outcome of the compaction itself instead of a pre-ack that
                // would mask storage/projection failures (P1-4).
                match self
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
                    Ok(()) => {
                        send_event(
                            tx,
                            ServerMessage::CommandResponse {
                                command_id,
                                result: Ok(crate::api::CommandResult::Empty),
                            },
                        )
                        .await;
                        Ok(())
                    }
                    Err(error) => {
                        tracing::warn!(session_id, error = %error, "session.compact failed");
                        Err(ProtocolError::InvalidCommand(format!(
                            "session.compact failed: {error}"
                        )))
                    }
                }
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
