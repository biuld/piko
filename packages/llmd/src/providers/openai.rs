use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

use piko_protocol::model::{InputModality, ModelSummary};

use crate::auth::{AuthCredential, AuthError};

use super::{DeviceAuthInfo, OAuthFlow, Provider};

// ============================================================================
// Static OpenAI model catalog — mirrors pi-mono's openai.models.ts
// ============================================================================

fn openai_models() -> Vec<ModelSummary> {
    let text = vec![InputModality::Text];
    let text_image = vec![InputModality::Text, InputModality::Image];

    vec![
        ModelSummary {
            id: "gpt-5".into(),
            name: "GPT-5".into(),
            reasoning: true,
            input: text_image.clone(),
            context_window: 400_000,
            max_tokens: 128_000,
        },
        ModelSummary {
            id: "gpt-5-chat-latest".into(),
            name: "GPT-5 Chat Latest".into(),
            reasoning: true,
            input: text_image.clone(),
            context_window: 400_000,
            max_tokens: 128_000,
        },
        ModelSummary {
            id: "gpt-5-codex".into(),
            name: "GPT-5 Codex".into(),
            reasoning: true,
            input: text.clone(),
            context_window: 400_000,
            max_tokens: 128_000,
        },
        ModelSummary {
            id: "gpt-5-mini".into(),
            name: "GPT-5 mini".into(),
            reasoning: false,
            input: text.clone(),
            context_window: 256_000,
            max_tokens: 64_000,
        },
        ModelSummary {
            id: "gpt-5-nano".into(),
            name: "GPT-5 nano".into(),
            reasoning: false,
            input: text.clone(),
            context_window: 256_000,
            max_tokens: 64_000,
        },
        ModelSummary {
            id: "o4-mini".into(),
            name: "o4 mini".into(),
            reasoning: true,
            input: text.clone(),
            context_window: 200_000,
            max_tokens: 100_000,
        },
        ModelSummary {
            id: "o3".into(),
            name: "o3".into(),
            reasoning: true,
            input: text.clone(),
            context_window: 200_000,
            max_tokens: 100_000,
        },
        ModelSummary {
            id: "gpt-4o".into(),
            name: "GPT-4o".into(),
            reasoning: false,
            input: text_image.clone(),
            context_window: 128_000,
            max_tokens: 16_384,
        },
        ModelSummary {
            id: "gpt-4o-mini".into(),
            name: "GPT-4o mini".into(),
            reasoning: false,
            input: text_image.clone(),
            context_window: 128_000,
            max_tokens: 16_384,
        },
        ModelSummary {
            id: "gpt-4.1".into(),
            name: "GPT-4.1".into(),
            reasoning: false,
            input: text_image.clone(),
            context_window: 1_047_576,
            max_tokens: 32_768,
        },
        ModelSummary {
            id: "gpt-4.1-mini".into(),
            name: "GPT-4.1 mini".into(),
            reasoning: false,
            input: text_image.clone(),
            context_window: 1_047_576,
            max_tokens: 32_768,
        },
        ModelSummary {
            id: "gpt-4.1-nano".into(),
            name: "GPT-4.1 nano".into(),
            reasoning: false,
            input: text_image.clone(),
            context_window: 1_047_576,
            max_tokens: 32_768,
        },
        ModelSummary {
            id: "gpt-4-turbo".into(),
            name: "GPT-4 Turbo".into(),
            reasoning: false,
            input: text_image.clone(),
            context_window: 128_000,
            max_tokens: 4_096,
        },
    ]
}

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

    /// Static model catalog — mirrors pi-mono's openai.models.ts.
    pub fn models() -> Vec<ModelSummary> {
        openai_models()
    }
}

#[async_trait::async_trait]
impl Provider for OpenAIProvider {
    fn id(&self) -> &str {
        "openai"
    }

    fn adapter_kind(&self) -> genai::adapter::AdapterKind {
        genai::adapter::AdapterKind::OpenAI
    }

    fn list_models(&self) -> Vec<ModelSummary> {
        Self::models()
    }

    fn oauth(&self) -> Option<&dyn OAuthFlow> {
        Some(self)
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
