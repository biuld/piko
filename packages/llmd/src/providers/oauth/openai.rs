use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

use crate::auth::{AuthCredential, AuthError};

use super::{DeviceAuthInfo, OAuthFlow, ProviderRequestAuth};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEVICE_USER_CODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const DEVICE_VERIFICATION_URI: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const DEFAULT_DEVICE_EXPIRES_SECONDS: u64 = 15 * 60;

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_auth_id: String,
    user_code: String,
    interval: Option<serde_json::Value>,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    authorization_code: Option<String>,
    code_verifier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    id_token: Option<String>,
}

/// OpenAI OAuth device-code flow handler.
/// Registered in ProviderRegistry's oauth_flows map.
pub struct OpenAIOAuthFlow {
    client: Client,
}

impl Default for OpenAIOAuthFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAIOAuthFlow {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl OAuthFlow for OpenAIOAuthFlow {
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
            expires_in_seconds: data.expires_in.unwrap_or(DEFAULT_DEVICE_EXPIRES_SECONDS),
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
                let data: DeviceTokenResponse = res.json().await.map_err(|e| AuthError::Io {
                    path: Default::default(),
                    source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
                })?;

                if let (Some(code), Some(verifier)) = (data.authorization_code, data.code_verifier)
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
                source: std::io::Error::other(format!("Token exchange failed ({status}): {body}")),
            });
        }

        let data: TokenExchangeResponse = res.json().await.map_err(|e| AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;

        let now = now_ms();
        let expires = refresh_at(now, data.expires_in);
        let extra = token_metadata(
            data.id_token
                .as_deref()
                .or(Some(data.access_token.as_str())),
        );

        Ok(AuthCredential::OAuth {
            access: data.access_token,
            refresh: data.refresh_token,
            expires: Some(expires),
            extra,
        })
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<AuthCredential, AuthError> {
        let res = self
            .client
            .post(TOKEN_URL)
            .json(&serde_json::json!({
                "client_id": CLIENT_ID,
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }))
            .send()
            .await
            .map_err(|error| provider_error(format!("refresh request failed: {error}")))?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(provider_error(format!(
                "token refresh failed ({status}): {body}"
            )));
        }

        let data: TokenExchangeResponse = res
            .json()
            .await
            .map_err(|error| provider_error(format!("invalid refresh response: {error}")))?;
        let now = now_ms();
        let extra = token_metadata(
            data.id_token
                .as_deref()
                .or(Some(data.access_token.as_str())),
        );
        Ok(AuthCredential::OAuth {
            access: data.access_token,
            refresh: Some(
                data.refresh_token
                    .unwrap_or_else(|| refresh_token.to_string()),
            ),
            expires: Some(refresh_at(now, data.expires_in)),
            extra,
        })
    }

    fn request_auth(&self, credential: &AuthCredential) -> Result<ProviderRequestAuth, AuthError> {
        let AuthCredential::OAuth { access, extra, .. } = credential else {
            return Err(provider_error(
                "expected an OAuth credential for subscription transport".into(),
            ));
        };
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), format!("Bearer {access}"));
        if let Some(account_id) = extra.get("account_id").and_then(|value| value.as_str()) {
            headers.insert("ChatGPT-Account-Id".to_string(), account_id.to_string());
        }
        Ok(ProviderRequestAuth {
            headers,
            expires_at: None,
        })
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn refresh_at(now_ms: u64, expires_in_seconds: u64) -> u64 {
    now_ms
        .saturating_add(expires_in_seconds.saturating_mul(1_000))
        .saturating_sub(5 * 60 * 1_000)
}

fn token_metadata(id_token: Option<&str>) -> HashMap<String, serde_json::Value> {
    let mut extra = HashMap::new();
    if let Some(account_id) = id_token.and_then(account_id_from_jwt) {
        extra.insert(
            "account_id".to_string(),
            serde_json::Value::String(account_id),
        );
    }
    extra
}

fn account_id_from_jwt(token: &str) -> Option<String> {
    use base64::Engine as _;

    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")
        .or_else(|| claims.get("chatgpt_account_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn provider_error(message: String) -> AuthError {
    AuthError::Provider {
        provider: "openai".to_string(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_chatgpt_account_id_from_id_token() {
        use base64::Engine as _;

        let claims = serde_json::json!({
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "account-123"
            }
        });
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&claims).unwrap());
        let token = format!("header.{payload}.signature");
        assert_eq!(account_id_from_jwt(&token).as_deref(), Some("account-123"));
    }

    #[test]
    fn materializes_subscription_responses_transport() {
        let flow = OpenAIOAuthFlow::new();
        let credential = AuthCredential::OAuth {
            access: "access-token".into(),
            refresh: Some("refresh-token".into()),
            expires: Some(u64::MAX),
            extra: HashMap::from([(
                "account_id".into(),
                serde_json::Value::String("account-123".into()),
            )]),
        };
        let request = flow.request_auth(&credential).unwrap();
        assert_eq!(
            request.headers.get("Authorization").map(String::as_str),
            Some("Bearer access-token")
        );
        assert_eq!(
            request
                .headers
                .get("ChatGPT-Account-Id")
                .map(String::as_str),
            Some("account-123")
        );
    }

    #[test]
    fn refresh_window_saturates_for_short_lived_tokens() {
        assert_eq!(refresh_at(100, 60), 0);
    }
}
