use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthCredential, AuthError};

pub mod openai;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuthInfo {
    pub device_auth_id: String,
    pub user_code: String,
    pub interval_seconds: u64,
    pub verification_uri: String,
}

#[async_trait]
pub trait OAuthProvider: Send + Sync {
    /// Return the provider ID, e.g. "openai"
    fn id(&self) -> &'static str;

    // ---- Device Code Flow ----
    
    /// Request a new device code and user code
    async fn start_device_auth(&self) -> Result<DeviceAuthInfo, AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Device auth not supported for {}", self.id()),
            ),
        })
    }

    /// Poll the provider until the user completes the flow
    async fn poll_device_auth(&self, _info: &DeviceAuthInfo) -> Result<(String, String), AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Device auth not supported for {}", self.id()),
            ),
        })
    }

    /// Exchange the authorization code for final tokens
    async fn exchange_code(&self, _code: String, _verifier: String) -> Result<AuthCredential, AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Code exchange not supported for {}", self.id()),
            ),
        })
    }

    // ---- Refresh Flow ----

    /// Refresh an expired token
    async fn refresh_token(&self, _refresh_token: &str) -> Result<AuthCredential, AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Token refresh not supported for {}", self.id()),
            ),
        })
    }
}

/// Registry of all supported OAuth providers
pub struct ProviderRegistry {
    providers: std::collections::HashMap<String, Box<dyn OAuthProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            providers: std::collections::HashMap::new(),
        };
        
        // Register known providers here
        registry.register(Box::new(openai::OpenAIOAuth::new()));
        
        registry
    }

    pub fn register(&mut self, provider: Box<dyn OAuthProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn get(&self, id: &str) -> Option<&dyn OAuthProvider> {
        self.providers.get(id).map(|p| p.as_ref())
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
