use std::collections::BTreeSet;

use crate::gateway::ModelRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum InputModality {
    Text,
    Image,
    Audio,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum OutputModality {
    Text,
    Audio,
    Image,
}

/// Open semantic identifier for a provider-hosted tool capability.
///
/// The value is catalog data rather than a closed Rust enum so adding a new
/// provider tool does not require changing llmd, orchd, or protocol code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct UpstreamToolKind(String);

impl UpstreamToolKind {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            || value.as_bytes()[0].is_ascii_digit()
        {
            return Err(format!(
                "upstream tool kind must be 1-64 lowercase ASCII letters, digits, or underscores and must not start with a digit: {value}"
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UpstreamToolKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum ToolExecutionLocus {
    Caller,
    Upstream,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum DeliveryCapability {
    Streaming,
    Assembled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum StateCapability {
    FullReplay,
    ServerContinuation,
    OpaqueReplay,
    ProviderCompaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum ExecutionCapability {
    Foreground,
    Durable,
    ResumableStream,
    Polling,
    Cancellation,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct ReasoningCapabilities {
    pub efforts: BTreeSet<piko_protocol::model::ThinkingLevel>,
    pub summaries: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct ToolCapabilities {
    pub loci: BTreeSet<ToolExecutionLocus>,
    pub parallel_calls: bool,
    pub required_choice: bool,
    pub specific_choice: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct StructuredOutputCapabilities {
    pub json_schema: bool,
    pub strict: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct ModelCapabilities {
    pub input_modalities: BTreeSet<InputModality>,
    pub output_modalities: BTreeSet<OutputModality>,
    pub tools: ToolCapabilities,
    pub reasoning: ReasoningCapabilities,
    pub structured_output: StructuredOutputCapabilities,
    pub delivery: BTreeSet<DeliveryCapability>,
    pub upstream_tools: BTreeSet<UpstreamToolKind>,
    pub state: BTreeSet<StateCapability>,
    pub execution: BTreeSet<ExecutionCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct ModelLimits {
    pub context_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub checkpoint_bytes: Option<u64>,
    pub tool_definition_bytes: Option<u64>,
    pub media_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModelDescriptor {
    pub model: ModelRef,
    pub display_name: String,
    pub capabilities: ModelCapabilities,
    pub limits: ModelLimits,
    /// Resolved model-facing upstream tools for this concrete target.
    pub upstream_tools: Vec<UpstreamToolDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UpstreamToolDescriptor {
    pub name: String,
    pub kind: UpstreamToolKind,
    pub approval: crate::tools::UpstreamApprovalPolicy,
    /// Opaque fingerprint of the provider-private request definition. Orchd
    /// uses it for prompt-prefix identity without learning the wire schema.
    pub wire_definition_digest: String,
}

impl crate::target::ModelTarget {
    pub fn descriptor(&self, provider: &str) -> ModelDescriptor {
        let mut capabilities = ModelCapabilities::default();
        if self.capabilities.text {
            capabilities.input_modalities.insert(InputModality::Text);
            capabilities.output_modalities.insert(OutputModality::Text);
        }
        if self.capabilities.images {
            capabilities.input_modalities.insert(InputModality::Image);
        }
        if self.capabilities.tools {
            capabilities.tools.loci.insert(ToolExecutionLocus::Caller);
        }
        if self.capabilities.upstream_dispatch && !self.capabilities.upstream_tools.is_empty() {
            capabilities.tools.loci.insert(ToolExecutionLocus::Upstream);
            capabilities.upstream_tools = self.capabilities.upstream_tools.clone();
        }
        if self.capabilities.upstream_dispatch && self.capabilities.hybrid_tools {
            capabilities.tools.loci.insert(ToolExecutionLocus::Hybrid);
        }
        capabilities.tools.parallel_calls = self.capabilities.parallel_tools;
        capabilities.tools.required_choice = self.capabilities.required_tool_choice;
        capabilities.tools.specific_choice = self.capabilities.specific_tool_choice;
        capabilities.reasoning.efforts = self.capabilities.reasoning_efforts.clone();
        capabilities.structured_output.json_schema = self.capabilities.structured_json_schema;
        capabilities.structured_output.strict = self.capabilities.strict_structured_output;
        if self.capabilities.streaming_delivery {
            capabilities.delivery.insert(DeliveryCapability::Streaming);
        }
        if self.capabilities.assembled_delivery {
            capabilities.delivery.insert(DeliveryCapability::Assembled);
        }
        if self.capabilities.replay_safe {
            capabilities.state.insert(StateCapability::FullReplay);
        }
        match self.responses_continuation() {
            Some(crate::modeling::ResponsesContinuationPolicy::PreviousResponseId) => {
                capabilities
                    .state
                    .insert(StateCapability::ServerContinuation);
            }
            Some(crate::modeling::ResponsesContinuationPolicy::EncryptedReasoning) => {
                capabilities.state.insert(StateCapability::OpaqueReplay);
            }
            _ => {}
        }
        capabilities
            .execution
            .insert(ExecutionCapability::Foreground);
        ModelDescriptor {
            model: ModelRef::new(provider, &self.model),
            display_name: self.model.clone(),
            capabilities,
            limits: ModelLimits {
                output_tokens: self.capabilities.max_output_tokens.map(u64::from),
                checkpoint_bytes: Some(96 * 1024),
                ..Default::default()
            },
            upstream_tools: self
                .upstream_tool_catalog
                .values()
                .map(|tool| UpstreamToolDescriptor {
                    name: tool.name.clone(),
                    kind: tool.kind.clone(),
                    approval: tool.approval,
                    wire_definition_digest: wire_definition_digest(&tool.wire_definition),
                })
                .collect(),
        }
    }
}

fn wire_definition_digest(definition: &serde_json::Value) -> String {
    use sha2::{Digest as _, Sha256};

    let encoded = serde_json::to_vec(definition)
        .expect("serializing an in-memory JSON tool definition cannot fail");
    format!("sha256:{:x}", Sha256::digest(encoded))
}
