use std::collections::HashMap;

use piko_protocol::model::ProviderAuthMethod;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::gateway::{ErrorClass, InferenceError, InferenceRequest};
use crate::modeling::{ProtocolProfile, ResponsesContinuationPolicy, TokenPricing};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub text: bool,
    pub images: bool,
    pub tools: bool,
    pub reasoning: bool,
    pub reasoning_efforts: std::collections::BTreeSet<piko_protocol::model::ThinkingLevel>,
    pub refusals: bool,
    pub upstream_tools: std::collections::BTreeSet<crate::capabilities::UpstreamToolKind>,
    pub hybrid_tools: bool,
    /// Internal adapter support gate, distinct from catalog discovery.
    pub upstream_dispatch: bool,
    pub parallel_tools: bool,
    pub required_tool_choice: bool,
    pub specific_tool_choice: bool,
    pub structured_json_schema: bool,
    pub strict_structured_output: bool,
    pub streaming_delivery: bool,
    pub assembled_delivery: bool,
    pub max_output_tokens: Option<u32>,
    /// Whether the complete semantic conversation is sufficient when no
    /// compatible provider checkpoint can be used.
    pub replay_safe: bool,
}

#[derive(Debug, Clone)]
pub struct ModelTargetConfig {
    pub target_id: String,
    pub api_surface: String,
    pub auth_method: ProviderAuthMethod,
    pub protocol: ProtocolProfile,
    pub capabilities: Option<ModelCapabilities>,
    pub base_url: Option<String>,
    pub endpoint: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub streaming_fallback: bool,
    pub reasoning_effort_map:
        std::collections::BTreeMap<piko_protocol::model::ThinkingLevel, String>,
    pub pricing: Option<TokenPricing>,
}

impl ModelTargetConfig {
    pub fn new(
        target_id: impl Into<String>,
        api_surface: impl Into<String>,
        auth_method: ProviderAuthMethod,
        protocol: ProtocolProfile,
    ) -> Self {
        Self {
            target_id: target_id.into(),
            api_surface: api_surface.into(),
            auth_method,
            protocol,
            capabilities: None,
            base_url: None,
            endpoint: None,
            headers: None,
            streaming_fallback: true,
            reasoning_effort_map: Default::default(),
            pricing: None,
        }
    }
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            text: true,
            images: true,
            tools: true,
            reasoning: true,
            reasoning_efforts: [
                piko_protocol::model::ThinkingLevel::Minimal,
                piko_protocol::model::ThinkingLevel::Low,
                piko_protocol::model::ThinkingLevel::Medium,
                piko_protocol::model::ThinkingLevel::High,
                piko_protocol::model::ThinkingLevel::XHigh,
            ]
            .into_iter()
            .collect(),
            refusals: true,
            upstream_tools: Default::default(),
            hybrid_tools: false,
            upstream_dispatch: false,
            parallel_tools: true,
            required_tool_choice: true,
            specific_tool_choice: true,
            structured_json_schema: false,
            strict_structured_output: false,
            streaming_delivery: true,
            assembled_delivery: true,
            max_output_tokens: None,
            replay_safe: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelTarget {
    pub id: String,
    pub api_surface: String,
    pub auth_method: ProviderAuthMethod,
    pub protocol: ProtocolProfile,
    pub endpoint: reqwest::Url,
    pub model: String,
    pub headers: HeaderMap,
    pub capabilities: ModelCapabilities,
    pub streaming_fallback: bool,
    pub reasoning_effort_map:
        std::collections::BTreeMap<piko_protocol::model::ThinkingLevel, String>,
    pub pricing: Option<TokenPricing>,
}

impl ModelTarget {
    pub fn resolve(
        id: &str,
        model: &str,
        config: &ModelTargetConfig,
        auth_headers: Option<&HashMap<String, String>>,
    ) -> Result<Self, InferenceError> {
        let protocol = config.protocol;
        let endpoint = if let Some(endpoint) = config.endpoint.as_deref() {
            reqwest::Url::parse(endpoint).map_err(|error| {
                InferenceError::new(
                    ErrorClass::Target,
                    id,
                    "resolve_target",
                    format!("invalid endpoint: {error}"),
                )
            })?
        } else {
            let base = config.base_url.as_deref().ok_or_else(|| {
                InferenceError::new(
                    ErrorClass::Target,
                    id,
                    "resolve_target",
                    "target has neither an endpoint nor an API-surface base URL",
                )
            })?;
            let mut base_url = reqwest::Url::parse(base).map_err(|error| {
                InferenceError::new(
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
            let operation = protocol.operation();
            base_url.join(operation).map_err(|error| {
                InferenceError::new(
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
                    return Err(InferenceError::new(
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
            id: config.target_id.clone(),
            api_surface: config.api_surface.clone(),
            auth_method: config.auth_method,
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
                    reasoning_efforts: capabilities.reasoning_efforts.clone(),
                    refusals: capabilities.refusals,
                    upstream_tools: capabilities.upstream_tools.clone(),
                    hybrid_tools: capabilities.hybrid_tools,
                    upstream_dispatch: capabilities.upstream_dispatch,
                    parallel_tools: capabilities.parallel_tools,
                    required_tool_choice: capabilities.required_tool_choice,
                    specific_tool_choice: capabilities.specific_tool_choice,
                    structured_json_schema: capabilities.structured_json_schema,
                    strict_structured_output: capabilities.strict_structured_output,
                    streaming_delivery: capabilities.streaming_delivery,
                    assembled_delivery: capabilities.assembled_delivery,
                    max_output_tokens: capabilities.max_output_tokens,
                    replay_safe: capabilities.replay_safe,
                })
                .unwrap_or_default(),
            streaming_fallback: config.streaming_fallback,
            reasoning_effort_map: config.reasoning_effort_map.clone(),
            pricing: config.pricing.clone(),
        })
    }

    pub fn validate(&self, request: &InferenceRequest) -> Result<(), InferenceError> {
        if !self.capabilities.text {
            return Err(InferenceError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "text output is not supported by this target",
            ));
        }
        if request.tools.iter().any(|tool| {
            matches!(
                tool,
                crate::tools::InferenceTool::Caller(_) | crate::tools::InferenceTool::Hybrid { .. }
            )
        }) && !self.capabilities.tools
        {
            return Err(InferenceError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "tools are not supported by this target",
            ));
        }
        for tool in &request.tools {
            if matches!(
                tool,
                crate::tools::InferenceTool::Upstream(_)
                    | crate::tools::InferenceTool::Hybrid { .. }
            ) && !self.capabilities.upstream_dispatch
            {
                return Err(InferenceError::new(
                    ErrorClass::UnsupportedCapability,
                    &self.id,
                    "validate",
                    "upstream tool dispatch is not enabled for this target",
                ));
            }
            if matches!(tool, crate::tools::InferenceTool::Hybrid { .. })
                && !self.capabilities.hybrid_tools
            {
                return Err(InferenceError::new(
                    ErrorClass::UnsupportedCapability,
                    &self.id,
                    "validate",
                    "hybrid tool execution is not supported by this target",
                ));
            }
            if let Some(kind) = tool.upstream_kind()
                && !self.capabilities.upstream_tools.contains(&kind)
            {
                return Err(InferenceError::new(
                    ErrorClass::UnsupportedCapability,
                    &self.id,
                    "validate",
                    format!("upstream tool {kind:?} is not supported by this target"),
                ));
            }
            if matches!(
                tool,
                crate::tools::InferenceTool::Upstream(_)
                    | crate::tools::InferenceTool::Hybrid { .. }
            ) {
                let authorized = match tool {
                    crate::tools::InferenceTool::Upstream(definition)
                    | crate::tools::InferenceTool::Hybrid {
                        upstream: definition,
                        ..
                    } => definition.authorization.is_some(),
                    crate::tools::InferenceTool::Caller(_) => true,
                };
                if !authorized {
                    return Err(InferenceError::new(
                        ErrorClass::UnsupportedCapability,
                        &self.id,
                        "validate",
                        "upstream execution requires host authorization",
                    ));
                }
            }
        }
        match &request.options.tool_choice {
            crate::gateway::ToolChoice::Auto | crate::gateway::ToolChoice::None => {}
            crate::gateway::ToolChoice::Required if request.tools.is_empty() => {
                return Err(InferenceError::new(
                    ErrorClass::UnsupportedCapability,
                    &self.id,
                    "validate",
                    "required tool choice needs at least one tool definition",
                ));
            }
            crate::gateway::ToolChoice::Required if !self.capabilities.required_tool_choice => {
                return Err(InferenceError::new(
                    ErrorClass::UnsupportedCapability,
                    &self.id,
                    "validate",
                    "required tool choice is not supported by this target",
                ));
            }
            crate::gateway::ToolChoice::Specific(name) => {
                if !self.capabilities.specific_tool_choice {
                    return Err(InferenceError::new(
                        ErrorClass::UnsupportedCapability,
                        &self.id,
                        "validate",
                        "specific tool choice is not supported by this target",
                    ));
                }
                if !request.tools.iter().any(|tool| tool.name() == name) {
                    return Err(InferenceError::new(
                        ErrorClass::UnsupportedCapability,
                        &self.id,
                        "validate",
                        format!("requested tool {name} is not defined"),
                    ));
                }
            }
            crate::gateway::ToolChoice::Required => {}
        }
        if request.options.parallel_tools == Some(true) && !self.capabilities.parallel_tools {
            return Err(InferenceError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "parallel tool calls are not supported by this target",
            ));
        }
        if let Some(intent) = &request.options.structured_output
            && (!self.capabilities.structured_json_schema
                || (intent.strict && !self.capabilities.strict_structured_output))
        {
            return Err(InferenceError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "requested structured output is not supported by this target",
            ));
        }
        let delivery_supported = match request.options.delivery {
            crate::gateway::DeliveryMode::Streaming => self.capabilities.streaming_delivery,
            crate::gateway::DeliveryMode::Assembled => self.capabilities.assembled_delivery,
        };
        if !delivery_supported {
            return Err(InferenceError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "requested delivery mode is not supported by this target",
            ));
        }
        if let (Some(requested), Some(limit)) = (
            request.options.max_output_tokens,
            self.capabilities.max_output_tokens,
        ) && requested > limit
        {
            return Err(InferenceError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                format!("requested output token limit {requested} exceeds target limit {limit}"),
            ));
        }
        if let Some(effort) = request.options.reasoning_effort.as_ref()
            && *effort != piko_protocol::model::ThinkingLevel::Off
            && (!self.capabilities.reasoning
                || !self.capabilities.reasoning_efforts.contains(effort))
        {
            return Err(InferenceError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                format!(
                    "reasoning effort {} is not supported by this target",
                    effort.as_str()
                ),
            ));
        }
        let has_images = request
            .conversation
            .items
            .iter()
            .any(|item| match &item.kind {
                crate::gateway::ConversationItemKind::Context { content, .. }
                | crate::gateway::ConversationItemKind::User { content } => match content {
                    piko_protocol::MessageContent::String(_) => false,
                    piko_protocol::MessageContent::Blocks(blocks) => blocks
                        .iter()
                        .any(|block| matches!(block, piko_protocol::ContentBlock::Image { .. })),
                },
                crate::gateway::ConversationItemKind::Assistant { content }
                | crate::gateway::ConversationItemKind::ToolResult { content, .. } => content
                    .iter()
                    .any(|block| matches!(block, piko_protocol::ContentBlock::Image { .. })),
                crate::gateway::ConversationItemKind::ToolCall { .. }
                | crate::gateway::ConversationItemKind::UpstreamActivity(_)
                | crate::gateway::ConversationItemKind::Source(_)
                | crate::gateway::ConversationItemKind::Citation(_)
                | crate::gateway::ConversationItemKind::Artifact(_) => false,
            });
        if has_images && !self.capabilities.images {
            return Err(InferenceError::new(
                ErrorClass::UnsupportedCapability,
                &self.id,
                "validate",
                "image input is not supported by this target",
            ));
        }
        Ok(())
    }
}

impl ModelTarget {
    pub fn responses_continuation(&self) -> Option<ResponsesContinuationPolicy> {
        self.protocol.responses_continuation()
    }

    pub fn reasoning_effort(&self, effort: &piko_protocol::model::ThinkingLevel) -> Option<String> {
        self.reasoning_effort_map.get(effort).cloned().or_else(|| {
            (*effort != piko_protocol::model::ThinkingLevel::Off).then(|| effort.as_str().into())
        })
    }
}

fn insert_header(
    headers: &mut HeaderMap,
    name: &str,
    value: &str,
    target: &str,
) -> Result<(), InferenceError> {
    let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
        InferenceError::new(
            ErrorClass::Target,
            target,
            "resolve_target",
            format!("invalid header name: {error}"),
        )
    })?;
    let value = HeaderValue::from_str(value).map_err(|error| {
        InferenceError::new(
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
#[path = "target_tests.rs"]
mod tests;
