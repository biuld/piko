use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

use crate::auth::{AuthCredential, AuthError};

use super::{BrowserAuthInfo, DeviceAuthInfo, OAuthFlow, ProviderRequestAuth};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEVICE_USER_CODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const DEVICE_VERIFICATION_URI: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const DEFAULT_DEVICE_EXPIRES_SECONDS: u64 = 15 * 60;
const DEFAULT_BROWSER_EXPIRES_SECONDS: u64 = 10 * 60;
const DEFAULT_TOKEN_EXPIRES_SECONDS: u64 = 60 * 60;
const BROWSER_CALLBACK_PORTS: &[u16] = &[1455, 1457];

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
    #[serde(default = "default_token_expires_seconds")]
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

    fn browser_callback_ports(&self) -> &'static [u16] {
        BROWSER_CALLBACK_PORTS
    }

    async fn start_browser_auth(&self, redirect_uri: &str) -> Result<BrowserAuthInfo, AuthError> {
        use base64::Engine as _;
        use rand::RngCore as _;
        use sha2::Digest as _;

        let mut verifier_bytes = [0_u8; 64];
        rand::thread_rng().fill_bytes(&mut verifier_bytes);
        let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);
        let digest = sha2::Sha256::digest(code_verifier.as_bytes());
        let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

        let mut state_bytes = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut state_bytes);
        let state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(state_bytes);

        let mut url = reqwest::Url::parse("https://auth.openai.com/oauth/authorize")
            .map_err(|error| provider_error(format!("invalid authorization URL: {error}")))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", CLIENT_ID)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair(
                "scope",
                "openid profile email offline_access api.connectors.read api.connectors.invoke",
            )
            .append_pair("code_challenge", &code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("id_token_add_organizations", "true")
            .append_pair("codex_cli_simplified_flow", "true")
            .append_pair("state", &state)
            .append_pair("originator", "codex_cli_rs");

        Ok(BrowserAuthInfo {
            authorization_url: url.into(),
            redirect_uri: redirect_uri.to_string(),
            state,
            code_verifier,
            expires_in_seconds: DEFAULT_BROWSER_EXPIRES_SECONDS,
        })
    }

    async fn finish_browser_auth(
        &self,
        info: &BrowserAuthInfo,
        code: String,
    ) -> Result<AuthCredential, AuthError> {
        let res = self
            .client
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", info.redirect_uri.as_str()),
                ("client_id", CLIENT_ID),
                ("code_verifier", info.code_verifier.as_str()),
            ])
            .send()
            .await
            .map_err(|error| provider_error(format!("token exchange failed: {error}")))?;
        token_response(res, None).await
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

const fn default_token_expires_seconds() -> u64 {
    DEFAULT_TOKEN_EXPIRES_SECONDS
}

async fn token_response(
    res: reqwest::Response,
    previous_refresh: Option<&str>,
) -> Result<AuthCredential, AuthError> {
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(provider_error(format!(
            "token exchange failed ({status}): {body}"
        )));
    }
    let data: TokenExchangeResponse = res
        .json()
        .await
        .map_err(|error| provider_error(format!("invalid token response: {error}")))?;
    let extra = token_metadata(
        data.id_token
            .as_deref()
            .or(Some(data.access_token.as_str())),
    );
    Ok(AuthCredential::OAuth {
        access: data.access_token,
        refresh: data
            .refresh_token
            .or_else(|| previous_refresh.map(str::to_string)),
        expires: Some(refresh_at(now_ms(), data.expires_in)),
        extra,
    })
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

    #[tokio::test]
    async fn browser_login_uses_pkce_state_and_loopback_redirect() {
        let flow = OpenAIOAuthFlow::new();
        assert_eq!(flow.browser_callback_ports(), &[1455, 1457]);
        let info = flow
            .start_browser_auth("http://localhost:1455/auth/callback")
            .await
            .unwrap();
        let url = reqwest::Url::parse(&info.authorization_url).unwrap();
        let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(url.path(), "/oauth/authorize");
        assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some("http://localhost:1455/auth/callback")
        );
        assert_eq!(query.get("state"), Some(&info.state));
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert!(!info.code_verifier.is_empty());
    }
}
