use std::collections::HashMap;

use piko_protocol::config::{ModelProtocol, ResponsesContinuationPolicy};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::gateway::{ErrorClass, GatewayError, ModelRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub text: bool,
    pub images: bool,
    pub tools: bool,
    pub reasoning: bool,
    pub refusals: bool,
}

#[derive(Debug, Clone)]
pub struct ModelTargetConfig {
    pub protocol: ModelProtocol,
    pub capabilities: Option<ModelCapabilities>,
    pub base_url: Option<String>,
    pub endpoint: Option<String>,
    pub responses_continuation: ResponsesContinuationPolicy,
    pub headers: Option<HashMap<String, String>>,
    pub streaming_fallback: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            text: true,
            images: true,
            tools: true,
            reasoning: true,
            refusals: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelTarget {
    pub id: String,
    pub protocol: ModelProtocol,
    pub endpoint: reqwest::Url,
    pub model: String,
    pub headers: HeaderMap,
    pub capabilities: ModelCapabilities,
    pub streaming_fallback: bool,
    pub responses_continuation: piko_protocol::config::ResponsesContinuationPolicy,
}

impl ModelTarget {
    pub fn resolve(
        id: &str,
        model: &str,
        config: &ModelTargetConfig,
        auth_headers: Option<&HashMap<String, String>>,
    ) -> Result<Self, GatewayError> {
        let protocol = config.protocol;
        let endpoint = if let Some(endpoint) = config.endpoint.as_deref() {
            reqwest::Url::parse(endpoint).map_err(|error| {
                GatewayError::new(
                    ErrorClass::Target,
                    id,
                    "resolve_target",
                    format!("invalid endpoint: {error}"),
                )
            })?
        } else {
            let base = config
                .base_url
                .as_deref()
                .unwrap_or("https://api.openai.com/v1/");
            let mut base_url = reqwest::Url::parse(base).map_err(|error| {
                GatewayError::new(
                    ErrorClass::Target,
                    id,
                    "resolve_target",
                    format!("invalid base URL: {error}"),
                )
            })?;
            if !base_url.path().ends_with('/') {
                let path = format!("{}/", base_url.path());
                base_url.set_path(&path);
            }
            let operation = match protocol {
                ModelProtocol::Responses => "responses",
                ModelProtocol::ChatCompletions => "chat/completions",
            };
            base_url.join(operation).map_err(|error| {
                GatewayError::new(
                    ErrorClass::Target,
                    id,
                    "resolve_target",
                    format!("invalid endpoint: {error}"),
                )
            })?
        };

        let mut headers = HeaderMap::new();
        if let Some(custom) = &config.headers {
            for (name, value) in custom {
                let lower = name.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "authorization" | "content-length" | "accept" | "content-type"
                ) {
                    return Err(GatewayError::new(
                        ErrorClass::Target,
                        id,
                        "resolve_target",
                        format!("custom header {name} is protected"),
                    ));
                }
                insert_header(&mut headers, name, value, id)?;
            }
        }
        if let Some(auth) = auth_headers {
            for (name, value) in auth {
                // Authentication material is trusted and supersedes the API
                // key, but it still cannot own protocol or endpoint.
                insert_header(&mut headers, name, value, id)?;
            }
        }

        Ok(Self {
            id: id.into(),
            protocol,
            endpoint,
            model: model.into(),
            headers,
            capabilities: config
                .capabilities
                .as_ref()
                .map(|capabilities| ModelCapabilities {
                    text: capabilities.text,
                    images: capabilities.images,
                    tools: capabilities.tools,
                    reasoning: capabilities.reasoning,
                    refusals: capabilities.refusals,
                })
                .unwrap_or_default(),
            streaming_fallback: config.streaming_fallback,
            responses_continuation: config.responses_continuation,
        })
    }

    pub fn validate(&self, request: &ModelRequest) -> Result<(), GatewayError> {
        if !self.capabilities.text {
            return Err(GatewayError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "text output is not supported by this target",
            ));
        }
        if !request.tools.is_empty() && !self.capabilities.tools {
            return Err(GatewayError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "tools are not supported by this target",
            ));
        }
        if request
            .thinking
            .as_deref()
            .is_some_and(|thinking| thinking != "none")
            && !self.capabilities.reasoning
        {
            return Err(GatewayError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "reasoning is not supported by this target",
            ));
        }
        let has_images = request.transcript.iter().any(|message| match message {
            piko_protocol::Message::Context { content, .. }
            | piko_protocol::Message::User { content, .. } => match content {
                piko_protocol::MessageContent::String(_) => false,
                piko_protocol::MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .any(|block| matches!(block, piko_protocol::ContentBlock::Image { .. })),
            },
            piko_protocol::Message::Assistant { content, .. }
            | piko_protocol::Message::ToolResult { content, .. } => content
                .iter()
                .any(|block| matches!(block, piko_protocol::ContentBlock::Image { .. })),
            piko_protocol::Message::ToolCall { .. } => false,
        });
        if has_images && !self.capabilities.images {
            return Err(GatewayError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "image input is not supported by this target",
            ));
        }
        for message in &request.transcript {
            if let piko_protocol::Message::Assistant {
                continuation: Some(continuation),
                ..
            } = message
            {
                let matches = matches!(
                    (self.protocol, continuation.as_ref()),
                    (
                        ModelProtocol::Responses,
                        piko_protocol::ModelContinuation::Responses { .. }
                    ) | (
                        ModelProtocol::ChatCompletions,
                        piko_protocol::ModelContinuation::ChatCompletions { .. }
                    )
                );
                if !matches {
                    return Err(GatewayError::new(
                        ErrorClass::UnsupportedCapability,
                        &self.id,
                        "validate",
                        "transcript continuation belongs to a different protocol",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn insert_header(
    headers: &mut HeaderMap,
    name: &str,
    value: &str,
    target: &str,
) -> Result<(), GatewayError> {
    let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
        GatewayError::new(
            ErrorClass::Target,
            target,
            "resolve_target",
            format!("invalid header name: {error}"),
        )
    })?;
    let value = HeaderValue::from_str(value).map_err(|error| {
        GatewayError::new(
            ErrorClass::Target,
            target,
            "resolve_target",
            format!("invalid header value: {error}"),
        )
    })?;
    headers.insert(name, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(protocol: ModelProtocol) -> ModelTargetConfig {
        ModelTargetConfig {
            protocol,
            capabilities: None,
            base_url: Some("https://example.test/v1".into()),
            endpoint: None,
            responses_continuation: Default::default(),
            headers: None,
            streaming_fallback: true,
        }
    }

    #[test]
    fn protocol_alone_selects_operation_path() {
        let responses = ModelTarget::resolve(
            "custom",
            "same-model",
            &config(ModelProtocol::Responses),
            None,
        )
        .unwrap();
        let chat = ModelTarget::resolve(
            "custom",
            "same-model",
            &config(ModelProtocol::ChatCompletions),
            None,
        )
        .unwrap();
        assert_eq!(
            responses.endpoint.as_str(),
            "https://example.test/v1/responses"
        );
        assert_eq!(
            chat.endpoint.as_str(),
            "https://example.test/v1/chat/completions"
        );
    }

    #[test]
    fn explicit_endpoint_is_not_rewritten() {
        let mut config = config(ModelProtocol::Responses);
        config.endpoint = Some("https://example.test/custom/inference".into());
        let target = ModelTarget::resolve("custom/model", "gpt", &config, None).unwrap();
        assert_eq!(
            target.endpoint.as_str(),
            "https://example.test/custom/inference"
        );
    }

    #[test]
    fn custom_headers_cannot_override_auth() {
        let mut config = config(ModelProtocol::Responses);
        config.headers = Some(HashMap::from([("Authorization".into(), "stolen".into())]));
        assert!(ModelTarget::resolve("custom", "gpt", &config, None).is_err());
    }

    #[test]
    fn capabilities_fail_before_dispatch() {
        let mut unsupported = config(ModelProtocol::Responses);
        unsupported.capabilities = Some(ModelCapabilities {
            tools: false,
            ..Default::default()
        });
        let target = ModelTarget::resolve("custom", "gpt", &unsupported, None).unwrap();
        let mut request = crate::protocols::tests_support::semantic_request();
        request.provider = "custom".into();
        assert_eq!(
            target.validate(&request).unwrap_err().class,
            ErrorClass::UnsupportedCapability
        );
    }
}
