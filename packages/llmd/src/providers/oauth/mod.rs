use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::{AuthCredential, AuthError};

pub mod openai;
mod resolver;

pub use resolver::{RuntimeAuthResolver, StoredAuthResolver};

// ============================================================================
// OAuthFlow trait
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuthInfo {
    pub device_auth_id: String,
    pub user_code: String,
    pub interval_seconds: u64,
    pub verification_uri: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRequestAuth {
    pub headers: HashMap<String, String>,
    pub expires_at: Option<std::time::SystemTime>,
}

#[async_trait]
pub trait OAuthFlow: Send + Sync {
    /// Return the provider ID, e.g. "openai"
    fn provider_id(&self) -> &str;

    // ---- Device Code Flow ----

    /// Request a new device code and user code
    async fn start_device_auth(&self) -> Result<DeviceAuthInfo, AuthError> {
        Err(AuthError::Unsupported {
            provider: self.provider_id().to_string(),
            operation: "device authentication",
        })
    }

    /// Complete a device interaction and return durable credentials. Providers
    /// whose polling endpoint returns tokens directly override this method;
    /// authorization-code providers may use the default two-step completion.
    async fn finish_device_auth(&self, info: &DeviceAuthInfo) -> Result<AuthCredential, AuthError> {
        let (code, verifier) = self.poll_device_auth(info).await?;
        self.exchange_code(code, verifier).await
    }

    /// Optional authorization-code device-flow step.
    async fn poll_device_auth(
        &self,
        _info: &DeviceAuthInfo,
    ) -> Result<(String, String), AuthError> {
        Err(AuthError::Unsupported {
            provider: self.provider_id().to_string(),
            operation: "device authentication polling",
        })
    }

    /// Optional authorization-code exchange step.
    async fn exchange_code(
        &self,
        _code: String,
        _verifier: String,
    ) -> Result<AuthCredential, AuthError> {
        Err(AuthError::Unsupported {
            provider: self.provider_id().to_string(),
            operation: "authorization-code exchange",
        })
    }

    // ---- Refresh Flow ----

    /// Refresh an expired token
    async fn refresh_token(&self, _refresh_token: &str) -> Result<AuthCredential, AuthError> {
        Err(AuthError::Unsupported {
            provider: self.provider_id().to_string(),
            operation: "token refresh",
        })
    }

    /// Convert a valid OAuth credential into request transport authentication.
    fn request_auth(&self, _credential: &AuthCredential) -> Result<ProviderRequestAuth, AuthError> {
        Err(AuthError::Unsupported {
            provider: self.provider_id().to_string(),
            operation: "OAuth request materialization",
        })
    }
}
