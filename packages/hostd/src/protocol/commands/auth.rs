use crate::api::{ProtocolError, ServerMessage};
use crate::util::ClientEventSender;
use piko_llmd::providers::OAuthFlow;
use piko_protocol::{AuthFailureReason, OAuthLoginMode};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::protocol::HostServer;

#[path = "auth_callback.rs"]
mod callback;
use callback::receive_browser_callback;

fn server_response_ok(command_id: &str, result: crate::api::CommandResult) -> ServerMessage {
    ServerMessage::CommandResponse {
        command_id: command_id.to_string(),
        result: Ok(result),
    }
}

impl HostServer {
    pub(crate) async fn start_oauth_login(
        &self,
        command_id: &str,
        provider: String,
        mode: OAuthLoginMode,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        let flow = {
            let reg = self.model_registry.lock().await;
            reg.get_oauth(&provider)
        }
        .ok_or_else(|| {
            ProtocolError::InvalidCommand(format!("OAuth not supported for provider {provider}"))
        })?;

        let login_id = uuid::Uuid::new_v4().to_string();
        let cancellation = CancellationToken::new();
        {
            let mut logins = self.auth_logins.lock().await;
            if logins.contains_key(&provider) {
                return Err(ProtocolError::InvalidCommand(format!(
                    "OAuth login already active for provider {provider}"
                )));
            }
            logins.insert(
                provider.clone(),
                crate::application::host_app::ActiveAuthLogin {
                    login_id: login_id.clone(),
                    cancellation: cancellation.clone(),
                },
            );
        }

        if let Err(error) = tx
            .send(server_response_ok(
                command_id,
                crate::api::CommandResult::AuthLoginStarted {
                    login_id: login_id.clone(),
                    provider: provider.clone(),
                    mode,
                    timestamp: crate::protocol::now_ms(),
                },
            ))
            .await
        {
            self.auth_logins.lock().await.remove(&provider);
            return Err(ProtocolError::InvalidCommand(error.to_string()));
        }

        let tx_clone = tx.clone();
        let server = self.clone();
        let registry = self.model_registry.clone();
        tokio::spawn(async move {
            let result = match mode {
                OAuthLoginMode::Browser => {
                    run_browser_login(
                        &login_id,
                        &provider,
                        flow.clone(),
                        &tx_clone,
                        cancellation.clone(),
                    )
                    .await
                }
                OAuthLoginMode::DeviceCode => {
                    run_device_login(
                        &login_id,
                        &provider,
                        flow.clone(),
                        &tx_clone,
                        cancellation.clone(),
                    )
                    .await
                }
            };

            match result {
                Ok(credential) => {
                    let still_active = server
                        .auth_logins
                        .lock()
                        .await
                        .get(&provider)
                        .is_some_and(|active| active.login_id == login_id);
                    if !still_active {
                        send_login_failure(
                            &tx_clone,
                            &login_id,
                            &provider,
                            AuthFailureReason::Cancelled,
                            "Login cancelled".into(),
                        )
                        .await;
                        return;
                    }
                    let stored = {
                        let mut reg = registry.lock().await;
                        reg.auth_storage_mut().set(&provider, credential)
                    };
                    if let Err(error) = stored {
                        send_login_failure(
                            &tx_clone,
                            &login_id,
                            &provider,
                            AuthFailureReason::Storage,
                            format!("Failed to store credentials: {error}"),
                        )
                        .await;
                    } else {
                        server.rebuild_agent_runner().await;
                        let _ = tx_clone
                            .send(ServerMessage::Auth(crate::api::AuthEvent::LoginSuccess {
                                login_id: Some(login_id.clone()),
                                provider: provider.clone(),
                            }))
                            .await;
                    }
                }
                Err((reason, error)) => {
                    send_login_failure(&tx_clone, &login_id, &provider, reason, error).await;
                }
            }

            let mut logins = server.auth_logins.lock().await;
            if logins
                .get(&provider)
                .is_some_and(|active| active.login_id == login_id)
            {
                logins.remove(&provider);
            }
        });
        Ok(())
    }

    pub(crate) async fn apply_auth_cancel_oauth(
        &self,
        command_id: &str,
        provider: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let active = self.auth_logins.lock().await.remove(&provider);
        let Some(active) = active else {
            return Err(ProtocolError::InvalidCommand(format!(
                "No active OAuth login for provider {provider}"
            )));
        };
        active.cancellation.cancel();
        Ok(vec![server_response_ok(
            command_id,
            crate::api::CommandResult::Empty,
        )])
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
        self.rebuild_agent_runner().await;

        Ok(vec![
            ServerMessage::Auth(crate::api::AuthEvent::LoginSuccess {
                login_id: None,
                provider,
            }),
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
        self.rebuild_agent_runner().await;

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

type LoginResult = Result<piko_llmd::auth::AuthCredential, (AuthFailureReason, String)>;

async fn run_browser_login(
    login_id: &str,
    provider: &str,
    flow: Arc<dyn OAuthFlow>,
    tx: &ClientEventSender,
    cancellation: CancellationToken,
) -> LoginResult {
    let listener = bind_browser_callback(flow.browser_callback_ports()).await?;
    let port = listener
        .local_addr()
        .map_err(|error| {
            (
                AuthFailureReason::Callback,
                format!("Failed to inspect OAuth callback: {error}"),
            )
        })?
        .port();
    let redirect_uri = format!("http://localhost:{port}/auth/callback");
    let info = flow
        .start_browser_auth(&redirect_uri)
        .await
        .map_err(|error| {
            (
                AuthFailureReason::Provider,
                format!("Failed to start browser authentication: {error}"),
            )
        })?;
    let _ = tx
        .send(ServerMessage::Auth(crate::api::AuthEvent::LoginBrowser {
            login_id: login_id.to_string(),
            provider: provider.to_string(),
            authorization_url: info.authorization_url.clone(),
        }))
        .await;

    let callback = tokio::select! {
        _ = cancellation.cancelled() => {
            return Err((AuthFailureReason::Cancelled, "Login cancelled".into()));
        }
        result = tokio::time::timeout(
            std::time::Duration::from_secs(info.expires_in_seconds),
            receive_browser_callback(listener, &info),
        ) => match result {
            Ok(result) => result?,
            Err(_) => return Err((AuthFailureReason::Expired, "Browser authorization expired".into())),
        },
    };
    flow.finish_browser_auth(&info, callback)
        .await
        .map_err(|error| (AuthFailureReason::Provider, error.to_string()))
}

async fn bind_browser_callback(ports: &[u16]) -> Result<TcpListener, (AuthFailureReason, String)> {
    let mut failures = Vec::new();
    for port in ports {
        match TcpListener::bind(("127.0.0.1", *port)).await {
            Ok(listener) => return Ok(listener),
            Err(error) => failures.push(format!("{port}: {error}")),
        }
    }
    Err((
        AuthFailureReason::Callback,
        format!(
            "Failed to bind a provider-approved OAuth callback port ({}). Try device login instead",
            failures.join(", ")
        ),
    ))
}

async fn run_device_login(
    login_id: &str,
    provider: &str,
    flow: Arc<dyn OAuthFlow>,
    tx: &ClientEventSender,
    cancellation: CancellationToken,
) -> LoginResult {
    let info = flow.start_device_auth().await.map_err(|error| {
        (
            AuthFailureReason::Provider,
            format!("Failed to start device authentication: {error}"),
        )
    })?;
    let _ = tx
        .send(ServerMessage::Auth(
            crate::api::AuthEvent::LoginDeviceCode {
                login_id: login_id.to_string(),
                provider: provider.to_string(),
                user_code: info.user_code.clone(),
                verification_uri: info.verification_uri.clone(),
            },
        ))
        .await;
    tokio::select! {
        _ = cancellation.cancelled() => {
            Err((AuthFailureReason::Cancelled, "Login cancelled".into()))
        }
        result = tokio::time::timeout(
            std::time::Duration::from_secs(info.expires_in_seconds),
            flow.finish_device_auth(&info),
        ) => match result {
            Ok(Ok(credential)) => Ok(credential),
            Ok(Err(error)) => Err((AuthFailureReason::Provider, error.to_string())),
            Err(_) => Err((AuthFailureReason::Expired, "Device authorization expired".into())),
        }
    }
}

async fn send_login_failure(
    tx: &ClientEventSender,
    login_id: &str,
    provider: &str,
    reason: AuthFailureReason,
    error: String,
) {
    let _ = tx
        .send(ServerMessage::Auth(crate::api::AuthEvent::LoginFailed {
            login_id: login_id.to_string(),
            provider: provider.to_string(),
            reason,
            error,
        }))
        .await;
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
