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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum HostedToolKind {
    Search,
    Retrieval,
    RemoteMcp,
    Shell,
    Computer,
    ImageGeneration,
    DeferredDiscovery,
    ProgrammaticExecution,
}

impl HostedToolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Retrieval => "retrieval",
            Self::RemoteMcp => "remote_mcp",
            Self::Shell => "shell",
            Self::Computer => "computer",
            Self::ImageGeneration => "image_generation",
            Self::DeferredDiscovery => "deferred_discovery",
            Self::ProgrammaticExecution => "programmatic_execution",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum ToolExecutionLocus {
    Caller,
    Provider,
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
    pub hosted_tools: BTreeSet<HostedToolKind>,
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
        if self.capabilities.hosted_dispatch && !self.capabilities.hosted_tools.is_empty() {
            capabilities.tools.loci.insert(ToolExecutionLocus::Provider);
            capabilities.hosted_tools = self.capabilities.hosted_tools.clone();
        }
        if self.capabilities.hosted_dispatch && self.capabilities.hybrid_tools {
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
        }
    }
}
