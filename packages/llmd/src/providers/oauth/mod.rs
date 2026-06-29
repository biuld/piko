use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthCredential, AuthError};

pub mod openai;

// ============================================================================
// OAuthFlow trait
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAuthInfo {
    pub device_auth_id: String,
    pub user_code: String,
    pub interval_seconds: u64,
    pub verification_uri: String,
}

#[async_trait]
pub trait OAuthFlow: Send + Sync {
    /// Return the provider ID, e.g. "openai"
    fn provider_id(&self) -> &str;

    // ---- Device Code Flow ----

    /// Request a new device code and user code
    async fn start_device_auth(&self) -> Result<DeviceAuthInfo, AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Device auth not supported for {}", self.provider_id()),
            ),
        })
    }

    /// Poll the provider until the user completes the flow
    async fn poll_device_auth(
        &self,
        _info: &DeviceAuthInfo,
    ) -> Result<(String, String), AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Device auth not supported for {}", self.provider_id()),
            ),
        })
    }

    /// Exchange the authorization code for final tokens
    async fn exchange_code(
        &self,
        _code: String,
        _verifier: String,
    ) -> Result<AuthCredential, AuthError> {
        Err(AuthError::Io {
            path: Default::default(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Code exchange not supported for {}", self.provider_id()),
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
                format!("Token refresh not supported for {}", self.provider_id()),
            ),
        })
    }
}
