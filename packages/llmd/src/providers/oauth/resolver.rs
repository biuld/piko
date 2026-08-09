use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::auth::{AuthCredential, AuthStorage};

use super::{OAuthFlow, ProviderRequestAuth};

#[async_trait]
pub trait RuntimeAuthResolver: Send + Sync {
    async fn resolve(&self, provider: &str) -> Result<Option<ProviderRequestAuth>, String>;
}

pub struct StoredAuthResolver {
    storage: Mutex<AuthStorage>,
    flows: HashMap<String, Arc<dyn OAuthFlow>>,
}

impl StoredAuthResolver {
    pub fn new(storage: AuthStorage, flows: HashMap<String, Arc<dyn OAuthFlow>>) -> Self {
        Self {
            storage: Mutex::new(storage),
            flows,
        }
    }
}

#[async_trait]
impl RuntimeAuthResolver for StoredAuthResolver {
    async fn resolve(&self, provider: &str) -> Result<Option<ProviderRequestAuth>, String> {
        let mut storage = self.storage.lock().await;
        let credential = storage
            .resolve_credential(provider, self.flows.get(provider).map(AsRef::as_ref))
            .await
            .map_err(|error| error.to_string())?;
        match credential {
            Some(AuthCredential::ApiKey { key }) => Ok(Some(ProviderRequestAuth {
                headers: HashMap::from([("Authorization".into(), format!("Bearer {key}"))]),
                expires_at: None,
            })),
            Some(credential @ AuthCredential::OAuth { .. }) => {
                let flow = self.flows.get(provider).ok_or_else(|| {
                    format!("no OAuth implementation registered for provider {provider}")
                })?;
                flow.request_auth(&credential)
                    .map(Some)
                    .map_err(|error| error.to_string())
            }
            None => Ok(None),
        }
    }
}

/// Compatibility alias for callers compiled against the OAuth-only name.
pub type StoredOAuthResolver = StoredAuthResolver;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::auth::AuthError;

    struct ExampleFlow {
        refreshes: AtomicUsize,
    }

    #[async_trait]
    impl OAuthFlow for ExampleFlow {
        fn provider_id(&self) -> &str {
            "example"
        }

        async fn refresh_token(&self, _refresh_token: &str) -> Result<AuthCredential, AuthError> {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            Ok(AuthCredential::OAuth {
                access: "fresh".into(),
                refresh: Some("rotated".into()),
                expires: Some(u64::MAX),
                extra: HashMap::new(),
            })
        }

        fn request_auth(
            &self,
            credential: &AuthCredential,
        ) -> Result<ProviderRequestAuth, AuthError> {
            Ok(ProviderRequestAuth {
                headers: HashMap::from([(
                    "Authorization".into(),
                    format!("Bearer {}", credential.secret()),
                )]),
                expires_at: None,
            })
        }
    }

    #[tokio::test]
    async fn resolves_each_request_but_refreshes_only_expired_credentials() {
        let flow = Arc::new(ExampleFlow {
            refreshes: AtomicUsize::new(0),
        });
        let storage = AuthStorage::in_memory(HashMap::from([(
            "example".into(),
            AuthCredential::OAuth {
                access: "expired".into(),
                refresh: Some("refresh".into()),
                expires: Some(0),
                extra: HashMap::new(),
            },
        )]));
        let resolver = StoredAuthResolver::new(
            storage,
            HashMap::from([("example".into(), Arc::clone(&flow) as Arc<dyn OAuthFlow>)]),
        );

        assert_eq!(
            resolver
                .resolve("example")
                .await
                .unwrap()
                .unwrap()
                .headers
                .get("Authorization")
                .map(String::as_str),
            Some("Bearer fresh")
        );
        assert_eq!(
            resolver
                .resolve("example")
                .await
                .unwrap()
                .unwrap()
                .headers
                .get("Authorization")
                .map(String::as_str),
            Some("Bearer fresh")
        );
        assert_eq!(flow.refreshes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn api_keys_use_the_same_header_only_runtime_auth_boundary() {
        let resolver = StoredAuthResolver::new(
            AuthStorage::in_memory(HashMap::from([(
                "example".into(),
                AuthCredential::ApiKey {
                    key: "secret".into(),
                },
            )])),
            HashMap::new(),
        );
        let auth = resolver.resolve("example").await.unwrap().unwrap();
        assert_eq!(
            auth.headers.get("Authorization").map(String::as_str),
            Some("Bearer secret")
        );
    }
}
