use llmd::auth::{AuthCredential, AuthSource, AuthStatus, AuthStorage};
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
fn oauth_credentials_resolve_to_access_token_without_provider_refresh() {
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

    assert_eq!(
        storage.get_api_key("github-copilot"),
        Some("access-token".into())
    );
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
    let resolved = storage.resolve_oauth_api_key("anthropic").await.unwrap();
    assert_eq!(resolved, Some("valid-token".into()));
}

#[tokio::test]
async fn oauth_resolve_returns_existing_token_if_expired_and_unknown_provider() {
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
    let resolved = storage
        .resolve_oauth_api_key("unknown-provider")
        .await
        .unwrap();
    assert_eq!(resolved, Some("expired-token".into()));
}
