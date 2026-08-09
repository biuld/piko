// ---- Protocol: model — public model config / capability types ----

use serde::{Deserialize, Serialize};

// ---- ThinkingLevel ----

/// User-facing semantic reasoning effort.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    #[serde(rename = "off")]
    #[default]
    Off,
    Minimal,
    Low,
    Medium,
    High,
    #[serde(rename = "xhigh")]
    XHigh,
}

impl ThinkingLevel {
    /// Return the canonical string representation for this level.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Off => "off",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }
}

// ---- InputModality ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum InputModality {
    Text,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum OutputModality {
    Text,
    Audio,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionLocus {
    Caller,
    Upstream,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum InferenceDeliveryMode {
    Streaming,
    Assembled,
}

// ---- ToolInfo ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

// ---- ModelCapabilities ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    #[serde(rename = "supportsTools")]
    pub supports_tools: bool,
    #[serde(rename = "supportsSandbox")]
    pub supports_sandbox: bool,
    #[serde(rename = "supportsMCP")]
    pub supports_mcp: bool,
    pub tools: Vec<ToolInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

// ---- ModelRunSettings ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRunSettings {
    #[serde(skip_serializing_if = "Option::is_none", rename = "parallelTools")]
    pub parallel_tools: Option<bool>,
    #[serde(rename = "allowToolCalls")]
    pub allow_tool_calls: bool,
    #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingLevel")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "toolChoice")]
    pub tool_choice: Option<ModelToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "stopConditions")]
    pub stop_conditions: Option<StopConditions>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "runtimeLimits")]
    pub runtime_limits: Option<ModelRuntimeLimits>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxTokens")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelToolChoice {
    Auto,
    None,
    Required,
    Specific { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StopConditions {
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "stopOnAssistantMessage"
    )]
    pub stop_on_assistant_message: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "stopOnToolResult")]
    pub stop_on_tool_result: Option<bool>,
}

// ---- ModelRuntimeCounters ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRuntimeCounters {
    #[serde(rename = "modelCalls")]
    pub model_calls: u64,
    #[serde(rename = "toolCalls")]
    pub tool_calls: u64,
    #[serde(rename = "consecutiveErrors")]
    pub consecutive_errors: u64,
    #[serde(rename = "startedAt")]
    pub started_at: i64,
}

// ---- ModelRuntimeLimits ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRuntimeLimits {
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxModelCalls")]
    pub max_model_calls: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxToolCalls")]
    pub max_tool_calls: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxWallClockMs")]
    pub max_wall_clock_ms: Option<u64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "maxConsecutiveErrors"
    )]
    pub max_consecutive_errors: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "perToolTimeoutMs")]
    pub per_tool_timeout_ms: Option<u64>,
}

// ---- Model catalog ----

/// Presentation and capability metadata for one provider-scoped model.
/// The enclosing `ProviderInfo` supplies its provider identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    pub id: String,
    pub name: String,
    pub reasoning: bool,
    pub input: Vec<InputModality>,
    #[serde(rename = "contextWindow")]
    pub context_window: u64,
    #[serde(rename = "maxTokens")]
    pub max_tokens: u64,
    /// Closed semantic effort values supported by this model.
    #[serde(default, rename = "reasoningEfforts")]
    pub reasoning_efforts: Vec<ThinkingLevel>,
    #[serde(default)]
    pub output: Vec<OutputModality>,
    #[serde(default, rename = "toolExecutionLoci")]
    pub tool_execution_loci: Vec<ToolExecutionLocus>,
    #[serde(default, rename = "parallelToolCalls")]
    pub parallel_tool_calls: bool,
    #[serde(default, rename = "structuredOutput")]
    pub structured_output: bool,
    #[serde(default, rename = "deliveryModes")]
    pub delivery_modes: Vec<InferenceDeliveryMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub provider: String,
    pub models: Vec<ModelSummary>,
    pub has_auth: bool,
    #[serde(default)]
    pub auth_methods: Vec<ProviderAuthMethod>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthMethod {
    ApiKey,
    OAuth,
}

impl Default for ModelRunSettings {
    fn default() -> Self {
        Self {
            parallel_tools: None,
            allow_tool_calls: true,
            thinking_level: None,
            tool_choice: None,
            stop_conditions: None,
            runtime_limits: None,
            max_tokens: None,
        }
    }
}
