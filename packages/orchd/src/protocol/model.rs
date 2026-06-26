// ---- Protocol: model — public model config / capability types ----

use serde::{Deserialize, Serialize};

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

// ---- ModelProviderConfig ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none", rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "baseUrl")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
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
    pub thinking_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "toolChoice")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "stopConditions")]
    pub stop_conditions: Option<StopConditions>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "runtimeLimits")]
    pub runtime_limits: Option<ModelRuntimeLimits>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxTokens")]
    pub max_tokens: Option<u32>,
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
