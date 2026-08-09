use crate::api::{ProtocolError, ServerMessage};
use crate::util::ClientEventSender;

use crate::protocol::HostServer;

fn server_response_ok(command_id: &str, result: crate::api::CommandResult) -> ServerMessage {
    ServerMessage::CommandResponse {
        command_id: command_id.to_string(),
        result: Ok(result),
    }
}

impl HostServer {
    pub(crate) fn start_oauth_login(
        &self,
        command_id: &str,
        provider: String,
        tx: &ClientEventSender,
    ) {
        let command_id = command_id.to_string();
        let tx_clone = tx.clone();
        let server = self.clone();
        let registry = self.model_registry.clone();
        tokio::spawn(async move {
            let flow = {
                let reg = registry.lock().await;
                reg.get_oauth(&provider)
            };

            let Some(flow) = flow else {
                let _ = tx_clone
                    .send(ServerMessage::Auth(crate::api::AuthEvent::LoginFailed {
                        provider,
                        error: "OAuth not supported for this provider".into(),
                    }))
                    .await;
                return;
            };

            match flow.start_device_auth().await {
                Ok(info) => {
                    let _ = tx_clone
                        .send(ServerMessage::Auth(
                            crate::api::AuthEvent::LoginDeviceCode {
                                provider: provider.clone(),
                                user_code: info.user_code.clone(),
                                verification_uri: info.verification_uri.clone(),
                            },
                        ))
                        .await;

                    match tokio::time::timeout(
                        std::time::Duration::from_secs(info.expires_in_seconds),
                        flow.finish_device_auth(&info),
                    )
                    .await
                    {
                        Ok(Ok(cred)) => {
                            if let Err(e) = {
                                let mut reg = registry.lock().await;
                                let auth = reg.auth_storage_mut();
                                auth.set(&provider, cred)
                            } {
                                let _ = tx_clone
                                    .send(ServerMessage::Auth(crate::api::AuthEvent::LoginFailed {
                                        provider: provider.clone(),
                                        error: format!("Failed to store credentials: {e}"),
                                    }))
                                    .await;
                                return;
                            }

                            let providers = registry.lock().await.list_providers();

                            // Replace any ErrorAgentRunRunner installed pre-login.
                            server.rebuild_turn_runner().await;

                            let _ = tx_clone
                                .send(ServerMessage::Auth(crate::api::AuthEvent::LoginSuccess {
                                    provider: provider.clone(),
                                }))
                                .await;
                            let _ = tx_clone
                                .send(server_response_ok(
                                    &command_id,
                                    crate::api::CommandResult::ModelListed {
                                        providers,
                                        timestamp: crate::protocol::now_ms(),
                                    },
                                ))
                                .await;
                        }
                        Ok(Err(e)) => {
                            let _ = tx_clone
                                .send(ServerMessage::Auth(crate::api::AuthEvent::LoginFailed {
                                    provider: provider.clone(),
                                    error: format!("Device authentication failed: {e}"),
                                }))
                                .await;
                        }
                        Err(_) => {
                            let _ = tx_clone
                                .send(ServerMessage::Auth(crate::api::AuthEvent::LoginFailed {
                                    provider: provider.clone(),
                                    error: "Device authorization expired".into(),
                                }))
                                .await;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx_clone
                        .send(ServerMessage::Auth(crate::api::AuthEvent::LoginFailed {
                            provider: provider.clone(),
                            error: format!("Start failed: {e}"),
                        }))
                        .await;
                }
            }
        });
    }

    pub(crate) async fn apply_auth_set_api_key(
        &self,
        command_id: &str,
        provider: String,
        api_key: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let providers = {
            let mut registry = self.model_registry.lock().await;
            let auth = registry.auth_storage_mut();
            auth.set(
                &provider,
                piko_llmd::auth::AuthCredential::ApiKey { key: api_key },
            )
            .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;
            auth.flush()
                .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;
            registry.list_providers()
        };

        // Auth is durable; rebuild so turns no longer hit a stale error runner.
        self.rebuild_turn_runner().await;

        Ok(vec![
            ServerMessage::Auth(crate::api::AuthEvent::LoginSuccess { provider }),
            server_response_ok(
                command_id,
                crate::api::CommandResult::ModelListed {
                    providers,
                    timestamp: crate::protocol::now_ms(),
                },
            ),
        ])
    }

    pub(crate) async fn apply_auth_logout(
        &self,
        command_id: &str,
        provider: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let providers = {
            let mut registry = self.model_registry.lock().await;
            let auth = registry.auth_storage_mut();
            auth.remove(&provider)
                .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;
            auth.flush()
                .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;
            registry.list_providers()
        };

        // Dropping credentials may invalidate the active default provider.
        self.rebuild_turn_runner().await;

        Ok(vec![
            ServerMessage::Auth(crate::api::AuthEvent::LoggedOut { provider }),
            server_response_ok(
                command_id,
                crate::api::CommandResult::ModelListed {
                    providers,
                    timestamp: crate::protocol::now_ms(),
                },
            ),
        ])
    }
}
