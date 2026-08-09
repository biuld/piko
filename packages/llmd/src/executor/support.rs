use std::collections::HashMap;
use std::sync::Arc;

use super::{LlmdExecutor, adapter_kind};

impl LlmdExecutor {
    pub(super) async fn request_target(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<(genai::ServiceTarget, HashMap<String, String>), String> {
        if let Some(resolver) = &self.state.auth_resolver
            && let Some(auth) = resolver.resolve(provider).await?
        {
            let endpoint = genai::resolver::Endpoint::from_owned(Arc::<str>::from(auth.base_url));
            let model = genai::ModelIden::new(auth.adapter_kind, model);
            return Ok((
                genai::ServiceTarget {
                    endpoint,
                    auth: genai::resolver::AuthData::Key(auth.bearer_token),
                    model,
                },
                auth.headers,
            ));
        }
        let model_iden = genai::ModelIden::new(adapter_kind(provider), model);
        let target = self
            .state
            .client
            .resolve_service_target(model_iden)
            .await
            .map_err(|error| error.to_string())?;
        let headers = self
            .state
            .provider_headers
            .get(provider)
            .cloned()
            .unwrap_or_default();
        Ok((target, headers))
    }
}

pub(super) fn sanitized_options(options: Option<&genai::chat::ChatOptions>) -> serde_json::Value {
    let mut value = serde_json::to_value(options)
        .unwrap_or_else(|error| serde_json::json!({ "serializationError": error.to_string() }));
    redact_header_fields(&mut value);
    value
}

fn redact_header_fields(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for key in ["extra_headers", "extraHeaders", "headers"] {
                if map.contains_key(key) {
                    map.insert(
                        key.to_string(),
                        serde_json::Value::String("[redacted]".into()),
                    );
                }
            }
            for child in map.values_mut() {
                redact_header_fields(child);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                redact_header_fields(child);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct Resolver;

    #[async_trait]
    impl crate::providers::RuntimeAuthResolver for Resolver {
        async fn resolve(
            &self,
            _provider: &str,
        ) -> Result<Option<crate::providers::ProviderRequestAuth>, String> {
            Ok(Some(crate::providers::ProviderRequestAuth {
                bearer_token: "oauth-token".into(),
                adapter_kind: genai::adapter::AdapterKind::OpenAIResp,
                base_url: "https://example.test/codex/".into(),
                headers: HashMap::from([("Account-Id".into(), "account-1".into())]),
            }))
        }
    }

    #[tokio::test]
    async fn oauth_resolution_controls_adapter_endpoint_and_headers() {
        let executor =
            LlmdExecutor::from_providers(HashMap::new()).with_auth_resolver(Arc::new(Resolver));
        let (target, headers) = executor.request_target("openai", "model").await.unwrap();
        assert_eq!(
            target.model.adapter_kind,
            genai::adapter::AdapterKind::OpenAIResp
        );
        assert_eq!(target.endpoint.base_url(), "https://example.test/codex/");
        assert_eq!(
            headers.get("Account-Id").map(String::as_str),
            Some("account-1")
        );
    }

    #[test]
    fn telemetry_options_redact_headers() {
        let options = genai::chat::ChatOptions::default().with_extra_headers(genai::Headers::from(
            ("Account-Id".to_string(), "account-1".to_string()),
        ));
        let serialized = sanitized_options(Some(&options)).to_string();
        assert!(!serialized.contains("account-1"));
        assert!(serialized.contains("redacted"));
    }
}
