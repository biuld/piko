use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

use crate::auth::{AuthCredential, AuthError};

use super::{DeviceAuthInfo, OAuthFlow};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEVICE_USER_CODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const DEVICE_VERIFICATION_URI: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_auth_id: String,
    user_code: String,
    interval: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    authorization_code: Option<String>,
    code_verifier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

/// OpenAI OAuth device-code flow handler.
/// Registered in ProviderRegistry's oauth_flows map.
pub struct OpenAIProvider {
    client: Client,
}

impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAIProvider {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl OAuthFlow for OpenAIProvider {
    fn provider_id(&self) -> &str {
        "openai"
    }

    async fn start_device_auth(&self) -> Result<DeviceAuthInfo, AuthError> {
        let res = self
            .client
            .post(DEVICE_USER_CODE_URL)
            .json(&serde_json::json!({ "client_id": CLIENT_ID }))
            .send()
            .await
            .map_err(|e| AuthError::Io {
                path: Default::default(),
                source: std::io::Error::other(e),
            })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(AuthError::Io {
                path: Default::default(),
                source: std::io::Error::other(format!(
                    "Device code request failed ({status}): {body}"
                )),
            });
        }

        let data: DeviceCodeResponse = res.json().await.map_err(|e| AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;

        let interval_seconds = match data.interval {
            Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(5),
            Some(serde_json::Value::String(s)) => s.parse().unwrap_or(5),
            _ => 5,
        };

        Ok(DeviceAuthInfo {
            device_auth_id: data.device_auth_id,
            user_code: data.user_code,
            interval_seconds,
            verification_uri: DEVICE_VERIFICATION_URI.to_string(),
        })
    }

    async fn poll_device_auth(&self, info: &DeviceAuthInfo) -> Result<(String, String), AuthError> {
        let mut interval = Duration::from_secs(info.interval_seconds);

        loop {
            sleep(interval).await;

            let res = self
                .client
                .post(DEVICE_TOKEN_URL)
                .json(&serde_json::json!({
                    "device_auth_id": info.device_auth_id,
                    "user_code": info.user_code,
                }))
                .send()
                .await
                .map_err(|e| AuthError::Io {
                    path: Default::default(),
                    source: std::io::Error::other(e),
                })?;

            if res.status().is_success() {
                let data: DeviceTokenResponse =
                    res.json().await.map_err(|e| AuthError::Io {
                        path: Default::default(),
                        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
                    })?;

                if let (Some(code), Some(verifier)) =
                    (data.authorization_code, data.code_verifier)
                {
                    return Ok((code, verifier));
                }
            } else if res.status() == 403 || res.status() == 404 {
                continue;
            } else {
                let body = res.text().await.unwrap_or_default();
                if body.contains("slow_down") {
                    interval += Duration::from_secs(5);
                    continue;
                }
                if body.contains("authorization_pending") {
                    continue;
                }
                return Err(AuthError::Io {
                    path: Default::default(),
                    source: std::io::Error::other(format!("Device auth failed: {body}")),
                });
            }
        }
    }

    async fn exchange_code(
        &self,
        code: String,
        verifier: String,
    ) -> Result<AuthCredential, AuthError> {
        let res = self
            .client
            .post(TOKEN_URL)
            .json(&serde_json::json!({
                "client_id": CLIENT_ID,
                "grant_type": "authorization_code",
                "code": code,
                "code_verifier": verifier,
                "redirect_uri": DEVICE_REDIRECT_URI,
            }))
            .send()
            .await
            .map_err(|e| AuthError::Io {
                path: Default::default(),
                source: std::io::Error::other(e),
            })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(AuthError::Io {
                path: Default::default(),
                source: std::io::Error::other(format!(
                    "Token exchange failed ({status}): {body}"
                )),
            });
        }

        let data: TokenExchangeResponse =
            res.json().await.map_err(|e| AuthError::Io {
                path: Default::default(),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
            })?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let expires = now + data.expires_in * 1000 - 5 * 60 * 1000;

        Ok(AuthCredential::OAuth {
            access: data.access_token,
            refresh: Some(data.refresh_token),
            expires: Some(expires),
            extra: std::collections::HashMap::new(),
        })
    }
}
