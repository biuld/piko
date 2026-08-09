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

pub struct StoredOAuthResolver {
    storage: Mutex<AuthStorage>,
    flows: HashMap<String, Arc<dyn OAuthFlow>>,
}

impl StoredOAuthResolver {
    pub fn new(storage: AuthStorage, flows: HashMap<String, Arc<dyn OAuthFlow>>) -> Self {
        Self {
            storage: Mutex::new(storage),
            flows,
        }
    }
}

#[async_trait]
impl RuntimeAuthResolver for StoredOAuthResolver {
    async fn resolve(&self, provider: &str) -> Result<Option<ProviderRequestAuth>, String> {
        let mut storage = self.storage.lock().await;
        if !matches!(storage.get(provider), Some(AuthCredential::OAuth { .. })) {
            return Ok(None);
        }
        let flow = self
            .flows
            .get(provider)
            .ok_or_else(|| format!("no OAuth implementation registered for provider {provider}"))?;
        let credential = storage
            .resolve_credential(provider, Some(flow.as_ref()))
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("no OAuth credential configured for provider {provider}"))?;
        flow.request_auth(&credential)
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

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
                bearer_token: credential.secret().into(),
                adapter_kind: genai::adapter::AdapterKind::OpenAI,
                base_url: "https://example.test/v1/".into(),
                headers: HashMap::new(),
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
        let resolver = StoredOAuthResolver::new(
            storage,
            HashMap::from([("example".into(), Arc::clone(&flow) as Arc<dyn OAuthFlow>)]),
        );

        assert_eq!(
            resolver
                .resolve("example")
                .await
                .unwrap()
                .unwrap()
                .bearer_token,
            "fresh"
        );
        assert_eq!(
            resolver
                .resolve("example")
                .await
                .unwrap()
                .unwrap()
                .bearer_token,
            "fresh"
        );
        assert_eq!(flow.refreshes.load(Ordering::SeqCst), 1);
    }
}
