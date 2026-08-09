use std::collections::HashMap;
use std::fs;
use std::io::Write;
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
    #[error("{operation} is not supported for provider {provider}")]
    Unsupported {
        provider: String,
        operation: &'static str,
    },
    #[error("OAuth credential for provider {provider} is expired and cannot be refreshed")]
    Expired { provider: String },
    #[error("provider {provider} authentication failed: {message}")]
    Provider { provider: String, message: String },
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
            Some(AuthCredential::OAuth { .. }) => None,
            None => env_api_key(provider),
        }
    }

    pub async fn resolve_credential(
        &mut self,
        provider: &str,
        oauth: Option<&dyn crate::providers::OAuthFlow>,
    ) -> Result<Option<AuthCredential>, AuthError> {
        if let Some(key) = self.runtime_overrides.get(provider) {
            return Ok(Some(AuthCredential::ApiKey { key: key.clone() }));
        }
        let Some(credential) = self.data.get(provider).cloned() else {
            return Ok(env_api_key(provider).map(|key| AuthCredential::ApiKey { key }));
        };
        let AuthCredential::OAuth {
            refresh,
            expires,
            extra: previous_extra,
            ..
        } = &credential
        else {
            return Ok(Some(credential));
        };
        if expires.is_some_and(|expires| now_ms() < expires) {
            return Ok(Some(credential));
        }
        let flow = oauth.ok_or_else(|| AuthError::Expired {
            provider: provider.to_string(),
        })?;
        let refresh_token = refresh
            .as_deref()
            .filter(|token| !token.is_empty())
            .ok_or_else(|| AuthError::Expired {
                provider: provider.to_string(),
            })?;
        let mut refreshed = flow.refresh_token(refresh_token).await?;
        if let AuthCredential::OAuth { extra, .. } = &mut refreshed {
            for (key, value) in previous_extra {
                extra.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
        self.set(provider, refreshed.clone())?;
        Ok(Some(refreshed))
    }
}

impl AuthCredential {
    pub fn secret(&self) -> &str {
        match self {
            Self::ApiKey { key } => key,
            Self::OAuth { access, .. } => access,
        }
    }

    pub fn is_oauth(&self) -> bool {
        matches!(self, Self::OAuth { .. })
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn read_file(path: &Path) -> Result<AuthStorageData, AuthError> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| AuthError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        write_file(path, &AuthStorageData::new())?;
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
    let temp_path = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    let write_result = (|| -> Result<(), std::io::Error> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp_path)?;
        file.write_all(encoded.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp_path, path)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result.map_err(|source| AuthError::Io {
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
