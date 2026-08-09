use piko_llmd::auth::{AuthCredential, AuthSource, AuthStatus, AuthStorage};
use std::collections::HashMap;
use std::fs;

#[test]
fn file_auth_storage_reads_and_writes_api_keys() {
    let temp = tempfile::tempdir().unwrap();
    let auth_path = temp.path().join("auth.json");
    fs::write(
        &auth_path,
        r#"{
          "openai": { "type": "api_key", "key": "stored-key" }
        }"#,
    )
    .unwrap();

    let mut storage = AuthStorage::create(Some(auth_path.clone())).unwrap();
    assert_eq!(storage.get_api_key("openai"), Some("stored-key".into()));
    assert_eq!(
        storage.get_auth_status("openai"),
        AuthStatus {
            configured: true,
            source: Some(AuthSource::Stored),
            label: None
        }
    );

    storage
        .set(
            "anthropic",
            AuthCredential::ApiKey {
                key: "anthropic-key".into(),
            },
        )
        .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&auth_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    let reloaded = AuthStorage::create(Some(auth_path)).unwrap();
    assert_eq!(
        reloaded.get_api_key("anthropic"),
        Some("anthropic-key".into())
    );
}

#[test]
fn runtime_api_key_overrides_stored_credentials() {
    let mut data = HashMap::new();
    data.insert(
        "openai".into(),
        AuthCredential::ApiKey {
            key: "stored".into(),
        },
    );
    let mut storage = AuthStorage::in_memory(data);
    storage.set_runtime_api_key("openai", "runtime");

    assert_eq!(storage.get_api_key("openai"), Some("runtime".into()));
}

#[test]
fn oauth_credentials_are_not_exposed_as_api_keys() {
    let mut data = HashMap::new();
    data.insert(
        "github-copilot".into(),
        AuthCredential::OAuth {
            access: "access-token".into(),
            refresh: Some("refresh-token".into()),
            expires: Some(123),
            extra: HashMap::new(),
        },
    );
    let storage = AuthStorage::in_memory(data);

    assert_eq!(storage.get_api_key("github-copilot"), None);
}

#[tokio::test]
async fn oauth_resolve_returns_access_token_if_not_expired() {
    let mut data = HashMap::new();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    data.insert(
        "anthropic".into(),
        AuthCredential::OAuth {
            access: "valid-token".into(),
            refresh: Some("refresh-token".into()),
            expires: Some(now_ms + 100_000),
            extra: HashMap::new(),
        },
    );
    let mut storage = AuthStorage::in_memory(data);
    let resolved = storage
        .resolve_credential("anthropic", piko_protocol::ProviderAuthMethod::OAuth, None)
        .await
        .unwrap();
    assert_eq!(
        resolved.as_ref().map(AuthCredential::secret),
        Some("valid-token")
    );
}

#[tokio::test]
async fn frozen_auth_route_rejects_a_different_credential_kind() {
    let mut storage = AuthStorage::in_memory(HashMap::from([(
        "example".into(),
        AuthCredential::ApiKey { key: "key".into() },
    )]));
    let error = storage
        .resolve_credential("example", piko_protocol::ProviderAuthMethod::OAuth, None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("target requires OAuth"));
}

#[tokio::test]
async fn oauth_resolve_rejects_expired_token_without_registered_flow() {
    let mut data = HashMap::new();
    data.insert(
        "unknown-provider".into(),
        AuthCredential::OAuth {
            access: "expired-token".into(),
            refresh: Some("refresh-token".into()),
            expires: Some(100),
            extra: HashMap::new(),
        },
    );
    let mut storage = AuthStorage::in_memory(data);
    let error = storage
        .resolve_credential(
            "unknown-provider",
            piko_protocol::ProviderAuthMethod::OAuth,
            None,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("expired"));
}

struct ExampleOAuthFlow;

#[async_trait::async_trait]
impl piko_llmd::providers::OAuthFlow for ExampleOAuthFlow {
    fn provider_id(&self) -> &str {
        "example"
    }

    async fn refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<AuthCredential, piko_llmd::auth::AuthError> {
        assert_eq!(refresh_token, "refresh-token");
        Ok(AuthCredential::OAuth {
            access: "fresh-token".into(),
            refresh: Some("rotated-token".into()),
            expires: Some(u64::MAX),
            extra: HashMap::new(),
        })
    }
}

#[tokio::test]
async fn oauth_refresh_is_delegated_to_registered_flow() {
    let mut data = HashMap::new();
    data.insert(
        "example".into(),
        AuthCredential::OAuth {
            access: "expired-token".into(),
            refresh: Some("refresh-token".into()),
            expires: Some(100),
            extra: HashMap::new(),
        },
    );
    let mut storage = AuthStorage::in_memory(data);
    let refreshed = storage
        .resolve_credential(
            "example",
            piko_protocol::ProviderAuthMethod::OAuth,
            Some(&ExampleOAuthFlow),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.secret(), "fresh-token");
}
