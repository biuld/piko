use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct AuthStore {
    pub providers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthCredential {
    ApiKey {
        key: String,
    },
    #[serde(rename = "oauth")]
    OAuth {
        access: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        refresh: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires: Option<u64>,
        #[serde(flatten)]
        extra: HashMap<String, serde_json::Value>,
    },
}

pub type AuthStorageData = HashMap<String, AuthCredential>;

#[derive(Debug, Clone, PartialEq)]
pub struct AuthStatus {
    pub configured: bool,
    pub source: Option<AuthSource>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthSource {
    Stored,
    Runtime,
    Environment,
}

#[derive(Debug, Clone)]
enum AuthBackend {
    File(PathBuf),
    Memory,
}

#[derive(Debug, Clone)]
pub struct AuthStorage {
    data: AuthStorageData,
    runtime_overrides: HashMap<String, String>,
    backend: AuthBackend,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("failed to read auth file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse auth file {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

impl AuthStorage {
    pub fn create(auth_path: Option<PathBuf>) -> Result<Self, AuthError> {
        let path = auth_path.unwrap_or_else(default_auth_path);
        let mut storage = Self {
            data: AuthStorageData::new(),
            runtime_overrides: HashMap::new(),
            backend: AuthBackend::File(path),
        };
        storage.reload()?;
        Ok(storage)
    }

    pub fn in_memory(data: AuthStorageData) -> Self {
        Self {
            data,
            runtime_overrides: HashMap::new(),
            backend: AuthBackend::Memory,
        }
    }

    pub fn reload(&mut self) -> Result<(), AuthError> {
        self.data = match &self.backend {
            AuthBackend::Memory => self.data.clone(),
            AuthBackend::File(path) => read_file(path)?,
        };
        Ok(())
    }

    pub fn flush(&self) -> Result<(), AuthError> {
        match &self.backend {
            AuthBackend::Memory => Ok(()),
            AuthBackend::File(path) => write_file(path, &self.data),
        }
    }

    pub fn set_runtime_api_key(&mut self, provider: impl Into<String>, api_key: impl Into<String>) {
        self.runtime_overrides
            .insert(provider.into(), api_key.into());
    }

    pub fn remove_runtime_api_key(&mut self, provider: &str) {
        self.runtime_overrides.remove(provider);
    }

    pub fn get(&self, provider: &str) -> Option<&AuthCredential> {
        self.data.get(provider)
    }

    pub fn set(
        &mut self,
        provider: impl Into<String>,
        credential: AuthCredential,
    ) -> Result<(), AuthError> {
        self.data.insert(provider.into(), credential);
        self.flush()
    }

    pub fn remove(&mut self, provider: &str) -> Result<(), AuthError> {
        self.data.remove(provider);
        self.flush()
    }

    pub fn list(&self) -> Vec<String> {
        self.data.keys().cloned().collect()
    }

    pub fn has(&self, provider: &str) -> bool {
        self.data.contains_key(provider)
    }

    pub fn has_auth(&self, provider: &str) -> bool {
        self.runtime_overrides.contains_key(provider)
            || self.data.contains_key(provider)
            || env_api_key(provider).is_some()
    }

    pub fn get_auth_status(&self, provider: &str) -> AuthStatus {
        if self.data.contains_key(provider) {
            return AuthStatus {
                configured: true,
                source: Some(AuthSource::Stored),
                label: None,
            };
        }
        if self.runtime_overrides.contains_key(provider) {
            return AuthStatus {
                configured: false,
                source: Some(AuthSource::Runtime),
                label: Some("--api-key".to_string()),
            };
        }
        if env_api_key(provider).is_some() {
            return AuthStatus {
                configured: false,
                source: Some(AuthSource::Environment),
                label: None,
            };
        }
        AuthStatus {
            configured: false,
            source: None,
            label: None,
        }
    }

    pub fn get_all(&self) -> AuthStorageData {
        self.data.clone()
    }

    pub fn get_api_key(&self, provider: &str) -> Option<String> {
        if let Some(key) = self.runtime_overrides.get(provider) {
            return Some(key.clone());
        }
        match self.data.get(provider) {
            Some(AuthCredential::ApiKey { key }) => Some(key.clone()),
            Some(AuthCredential::OAuth { access, .. }) => Some(access.clone()),
            None => env_api_key(provider),
        }
    }

    pub async fn resolve_oauth_api_key(
        &mut self,
        provider: &str,
    ) -> Result<Option<String>, String> {
        let cred = match self.data.get(provider) {
            Some(AuthCredential::OAuth {
                access,
                refresh,
                expires,
                extra,
            }) => {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let is_expired = expires.is_none_or(|exp| now_ms >= exp);
                if !is_expired {
                    return Ok(Some(access.clone()));
                }

                (refresh.clone(), extra.clone())
            }
            Some(AuthCredential::ApiKey { key }) => return Ok(Some(key.clone())),
            None => return Ok(env_api_key(provider)),
        };

        let (refresh_token, extra) = cred;
        let refresh_token = match refresh_token {
            Some(ref token) if !token.is_empty() => token,
            _ => {
                if let Some(AuthCredential::OAuth { access, .. }) = self.data.get(provider) {
                    return Ok(Some(access.clone()));
                }
                return Ok(None);
            }
        };

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let client = reqwest::Client::new();

        match provider {
            "anthropic" => {
                let client_id = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
                let token_url = "https://platform.claude.com/v1/oauth/token";

                let payload = serde_json::json!({
                    "grant_type": "refresh_token",
                    "client_id": client_id,
                    "refresh_token": refresh_token,
                });

                let res = client
                    .post(token_url)
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|e| format!("anthropic refresh request failed: {e}"))?;

                if !res.status().is_success() {
                    let status = res.status();
                    let body = res.text().await.unwrap_or_default();
                    return Err(format!("anthropic refresh failed status={status}: {body}"));
                }

                #[derive(Deserialize)]
                struct RefreshResponse {
                    access_token: String,
                    refresh_token: String,
                    expires_in: u64,
                }

                let resp: RefreshResponse = res
                    .json()
                    .await
                    .map_err(|e| format!("failed to parse anthropic refresh response: {e}"))?;

                let expires = now_ms + resp.expires_in * 1000 - 5 * 60 * 1000;

                let new_cred = AuthCredential::OAuth {
                    access: resp.access_token.clone(),
                    refresh: Some(resp.refresh_token),
                    expires: Some(expires),
                    extra,
                };

                self.set(provider, new_cred).map_err(|e| e.to_string())?;
                Ok(Some(resp.access_token))
            }
            "antigravity" => {
                let client_id = std::env::var("ANTI_GRAVITY_GOOGLE_CLIENT_ID").unwrap_or_default();
                let client_secret =
                    std::env::var("ANTI_GRAVITY_GOOGLE_CLIENT_SECRET").unwrap_or_default();
                let token_url = "https://oauth2.googleapis.com/token";

                let params: [(&str, &str); 4] = [
                    ("client_id", client_id.as_str()),
                    ("client_secret", client_secret.as_str()),
                    ("refresh_token", refresh_token),
                    ("grant_type", "refresh_token"),
                ];

                let res = client
                    .post(token_url)
                    .form(&params)
                    .send()
                    .await
                    .map_err(|e| format!("antigravity refresh request failed: {e}"))?;

                if !res.status().is_success() {
                    let status = res.status();
                    let body = res.text().await.unwrap_or_default();
                    return Err(format!(
                        "antigravity refresh failed status={status}: {body}"
                    ));
                }

                #[derive(Deserialize)]
                struct RefreshResponse {
                    access_token: String,
                    refresh_token: Option<String>,
                    expires_in: u64,
                }

                let resp: RefreshResponse = res
                    .json()
                    .await
                    .map_err(|e| format!("failed to parse antigravity refresh response: {e}"))?;

                let expires = now_ms + resp.expires_in * 1000 - 5 * 60 * 1000;
                let new_refresh = resp
                    .refresh_token
                    .unwrap_or_else(|| refresh_token.to_string());

                let new_cred = AuthCredential::OAuth {
                    access: resp.access_token.clone(),
                    refresh: Some(new_refresh),
                    expires: Some(expires),
                    extra,
                };

                self.set(provider, new_cred).map_err(|e| e.to_string())?;
                Ok(Some(resp.access_token))
            }
            "github-copilot" => {
                let enterprise_domain = extra
                    .get("enterpriseUrl")
                    .and_then(|v| v.as_str())
                    .unwrap_or("github.com");

                let copilot_token_url =
                    format!("https://api.{enterprise_domain}/copilot_internal/v2/token");

                let res = client
                    .get(&copilot_token_url)
                    .header("Accept", "application/json")
                    .header("Authorization", format!("Bearer {refresh_token}"))
                    .header("User-Agent", "GitHubCopilotChat/0.35.0")
                    .header("Editor-Version", "vscode/1.107.0")
                    .header("Editor-Plugin-Version", "copilot-chat/0.35.0")
                    .header("Copilot-Integration-Id", "vscode-chat")
                    .send()
                    .await
                    .map_err(|e| format!("github-copilot refresh request failed: {e}"))?;

                if !res.status().is_success() {
                    let status = res.status();
                    let body = res.text().await.unwrap_or_default();
                    return Err(format!(
                        "github-copilot refresh failed status={status}: {body}"
                    ));
                }

                #[derive(Deserialize)]
                struct RefreshResponse {
                    token: String,
                    expires_at: u64,
                }

                let resp: RefreshResponse = res
                    .json()
                    .await
                    .map_err(|e| format!("failed to parse github-copilot response: {e}"))?;

                let expires = resp.expires_at * 1000 - 5 * 60 * 1000;

                let new_cred = AuthCredential::OAuth {
                    access: resp.token.clone(),
                    refresh: Some(refresh_token.to_string()),
                    expires: Some(expires),
                    extra,
                };

                self.set(provider, new_cred).map_err(|e| e.to_string())?;
                Ok(Some(resp.token))
            }
            _ => {
                if let Some(AuthCredential::OAuth { access, .. }) = self.data.get(provider) {
                    return Ok(Some(access.clone()));
                }
                Ok(None)
            }
        }
    }
}

fn read_file(path: &Path) -> Result<AuthStorageData, AuthError> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| AuthError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(path, "{}").map_err(|source| AuthError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    }
    let content = fs::read_to_string(path).map_err(|source| AuthError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&content).map_err(|source| AuthError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn write_file(path: &Path, data: &AuthStorageData) -> Result<(), AuthError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AuthError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let encoded = serde_json::to_string_pretty(data).map_err(|source| AuthError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, encoded).map_err(|source| AuthError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn default_auth_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".piko").join("auth.json")
}

fn env_api_key(provider: &str) -> Option<String> {
    let upper = provider
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    for key in [
        format!("{upper}_API_KEY"),
        format!("PIKO_{upper}_API_KEY"),
        match provider {
            "openai" => "OPENAI_API_KEY".to_string(),
            "anthropic" => "ANTHROPIC_API_KEY".to_string(),
            _ => String::new(),
        },
    ] {
        if key.is_empty() {
            continue;
        }
        if let Ok(value) = std::env::var(key)
            && !value.is_empty()
        {
            return Some(value);
        }
    }
    None
}
