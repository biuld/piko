use tokio::sync::mpsc::UnboundedSender;

use crate::api::{ProtocolError, ServerMessage};

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
        tx: &UnboundedSender<ServerMessage>,
    ) {
        let command_id = command_id.to_string();
        let tx_clone = tx.clone();
        let registry = self.model_registry.clone();
        tokio::spawn(async move {
            let oauth = {
                let reg = registry.lock().await;
                reg.get_oauth(&provider).is_some()
            };

            if !oauth {
                let _ = tx_clone.send(ServerMessage::Auth(crate::api::AuthEvent::LoginFailed {
                    provider,
                    error: "OAuth not supported for this provider".into(),
                }));
                return;
            }

            let reg = registry.lock().await;
            let flow = match reg.get_oauth(&provider) {
                Some(f) => f,
                None => {
                    let _ =
                        tx_clone.send(ServerMessage::Auth(crate::api::AuthEvent::LoginFailed {
                            provider,
                            error: "OAuth not supported for this provider".into(),
                        }));
                    return;
                }
            };

            match flow.start_device_auth().await {
                Ok(info) => {
                    let _ = tx_clone.send(ServerMessage::Auth(
                        crate::api::AuthEvent::LoginDeviceCode {
                            provider: provider.clone(),
                            user_code: info.user_code.clone(),
                            verification_uri: info.verification_uri.clone(),
                        },
                    ));

                    match flow.poll_device_auth(&info).await {
                        Ok((code, verifier)) => match flow.exchange_code(code, verifier).await {
                            Ok(_cred) => {
                                let _ = tx_clone.send(ServerMessage::Auth(
                                    crate::api::AuthEvent::LoginSuccess {
                                        provider: provider.clone(),
                                    },
                                ));
                                let reg = registry.lock().await;
                                let providers = reg.list_providers();
                                let _ = tx_clone.send(server_response_ok(
                                    &command_id,
                                    crate::api::CommandResult::ModelListed {
                                        providers,
                                        timestamp: crate::protocol::now_ms(),
                                    },
                                ));
                            }
                            Err(e) => {
                                let _ = tx_clone.send(ServerMessage::Auth(
                                    crate::api::AuthEvent::LoginFailed {
                                        provider: provider.clone(),
                                        error: format!("Exchange failed: {e}"),
                                    },
                                ));
                            }
                        },
                        Err(e) => {
                            let _ = tx_clone.send(ServerMessage::Auth(
                                crate::api::AuthEvent::LoginFailed {
                                    provider: provider.clone(),
                                    error: format!("Poll failed: {e}"),
                                },
                            ));
                        }
                    }
                }
                Err(e) => {
                    let _ =
                        tx_clone.send(ServerMessage::Auth(crate::api::AuthEvent::LoginFailed {
                            provider: provider.clone(),
                            error: format!("Start failed: {e}"),
                        }));
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
        let mut registry = self.model_registry.lock().await;
        let auth = registry.auth_storage_mut();
        auth.set(
            &provider,
            llmd::auth::AuthCredential::ApiKey { key: api_key },
        )
        .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;
        auth.flush()
            .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;

        let providers = registry.list_providers();

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
        let mut registry = self.model_registry.lock().await;
        let auth = registry.auth_storage_mut();
        auth.remove(&provider)
            .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;
        auth.flush()
            .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;

        let providers = registry.list_providers();

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
