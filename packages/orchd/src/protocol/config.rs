// ---- Protocol: config — orchd startup configuration types ----
//
// Host provides this once at startup. orchd uses it to wire
// providers, agents, tool sets, and runtime limits.
//
// orchd knows nothing about env vars, keychains, sessions, or users.
// All external-world knowledge comes from this config.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::agents::{AgentSpec, AgentTaskId, TaskSource};
use super::messages::Message;
use super::model::ModelRunSettings;
use super::runtime::OrchestratorRuntimeConfig;

// ---- Provider configuration ----

/// Provider identifier (e.g. "openai", "anthropic").
pub type ProviderId = String;

/// Credentials + endpoint for one LLM provider.
///
/// The Host resolves API keys from its own auth storage before passing
/// them here. orchd never reads env vars or keychains.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// Provider kind: "openai", "anthropic", "openrouter", "gemini", etc.
    pub kind: String,

    /// Resolved API key (Host already decrypted / read from env).
    pub api_key: String,

    /// Custom base URL for OpenAI-compatible providers, local proxies, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    /// Extra HTTP headers (e.g. OpenRouter HTTP-Referer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

// ---- Model reference ----

/// Lightweight reference to a model within a configured provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    /// Must match a ProviderConfig.kind.
    pub provider: String,

    /// Model ID string (e.g. "claude-sonnet-4-5-20250929").
    pub model_id: String,
}

// ---- Full startup config ----

/// Complete orchd startup configuration.
///
/// Passed once by the Host after spawning the orchd process (or during
/// in-process construction). orchd wires everything from this and is
/// ready to process tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchdConfig {
    /// Available LLM providers with credentials.
    pub providers: HashMap<ProviderId, ProviderConfig>,

    /// Agent definitions. "main" agent is required.
    pub agents: HashMap<String, AgentSpec>,

    /// Default model for agents that don't specify one.
    pub default_model: ModelRef,

    /// Default run settings.
    #[serde(default)]
    pub default_settings: ModelRunSettings,

    /// Runtime-wide limits.
    #[serde(default)]
    pub runtime: OrchestratorRuntimeConfig,
}

impl Default for OrchdConfig {
    fn default() -> Self {
        Self {
            providers: HashMap::new(),
            agents: HashMap::new(),
            default_model: ModelRef {
                provider: "openai".into(),
                model_id: "gpt-4o".into(),
            },
            default_settings: ModelRunSettings::default(),
            runtime: OrchestratorRuntimeConfig::default(),
        }
    }
}

impl OrchdConfig {
    /// Minimal config with a single provider.
    pub fn single_provider(
        kind: impl Into<String>,
        api_key: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Self {
        let kind_str: String = kind.into();
        let model_id_str: String = model_id.into();
        let provider_config = ProviderConfig {
            kind: kind_str.clone(),
            api_key: api_key.into(),
            base_url: None,
            headers: None,
        };
        let mut providers = HashMap::new();
        providers.insert(kind_str.clone(), provider_config);

        let main_agent = AgentSpec {
            id: "main".into(),
            name: "Main".into(),
            role: "assistant".into(),
            description: Some("Default agent".into()),
            system_prompt: String::new(),
            model: Some(model_id_str.clone()),
            tool_set_ids: vec!["builtin".into(), "workspace".into()],
            active_tool_names: None,
        };
        let mut agents = HashMap::new();
        agents.insert("main".into(), main_agent);

        Self {
            providers,
            agents,
            default_model: ModelRef {
                provider: kind_str,
                model_id: model_id_str,
            },
            default_settings: ModelRunSettings::default(),
            runtime: OrchestratorRuntimeConfig::default(),
        }
    }
}

// ---- Task input ----

/// Input for a single run / spawn operation.
///
/// orchd creates an `AgentTask` from this internally, but the
/// TaskInput type provides a cleaner public API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    /// User prompt or task description.
    pub prompt: String,

    /// Target agent ID (defaults to "main").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<String>,

    /// Optional conversation history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,

    /// Per-task overrides for model / settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<TaskOverrides>,

    /// Parent task ID for sub-agent calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
}

impl TaskInput {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            target_agent_id: None,
            history: None,
            overrides: None,
            parent_task_id: None,
        }
    }

    pub fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.target_agent_id = Some(agent_id.into());
        self
    }

    pub fn with_history(mut self, history: Vec<Message>) -> Self {
        self.history = Some(history);
        self
    }

    pub fn convert_to_agent_task(&self, source: TaskSource) -> super::agents::AgentTask {
        super::agents::AgentTask {
            id: None,
            target_agent_id: self
                .target_agent_id
                .clone()
                .unwrap_or_else(|| "main".into()),
            prompt: self.prompt.clone(),
            source,
            priority: None,
            parent_task_id: self.parent_task_id.clone(),
            history: self.history.clone(),
            host_context: None,
        }
    }

    pub fn convert_to_agent_task_with_override(
        &self,
        source: TaskSource,
    ) -> super::agents::AgentTask {
        let mut task = self.convert_to_agent_task(source);

        // Add overrides as part of the model config stored in the task
        if let Some(overrides) = &self.overrides {
            task.prompt = json_merge_prompt(&task.prompt, overrides);
        }

        task
    }
}

fn json_merge_prompt(prompt: &str, _overrides: &TaskOverrides) -> String {
    // Overrides are handled via ModelConfig changes on the agent.
    prompt.to_string()
}

// ---- Task overrides ----

/// Per-task overrides that modify agent behaviour for a single run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskOverrides {
    /// Override the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,

    /// Override run settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<ModelRunSettings>,

    /// Append to the system prompt for this task only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt_append: Option<String>,
}

// ---- Task result ----

/// The final result of a task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    pub task_id: AgentTaskId,
    pub status: TaskStatus,
    pub messages: Vec<Message>,
    pub total_steps: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<super::messages::Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Completed,
    Aborted,
    Error,
}

// ---- User interaction types ----

/// Response from the Host to a user-interaction event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UserResponse {
    /// Response to ask_user.
    #[serde(rename = "ask_user")]
    AskUser { answer: String },

    /// Response to request_approval.
    #[serde(rename = "request_approval")]
    RequestApproval {
        approved: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

// ---- Error type ----

/// orchd-facing error type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchdError {
    pub code: String,
    pub message: String,
}

impl OrchdError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self {
            code: "config_error".into(),
            message: msg.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            code: "not_found".into(),
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: "internal".into(),
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for OrchdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for OrchdError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchd_error_constructors() {
        let e = OrchdError::config("bad config");
        assert_eq!(e.code, "config_error");
        assert_eq!(e.message, "bad config");

        let e = OrchdError::not_found("missing");
        assert_eq!(e.code, "not_found");

        let e = OrchdError::internal("boom");
        assert_eq!(e.code, "internal");
    }

    #[test]
    fn test_orchd_error_display() {
        let e = OrchdError::internal("test");
        assert_eq!(format!("{e}"), "internal: test");
    }

    #[test]
    fn test_task_input_new() {
        let input = TaskInput::new("hello");
        assert_eq!(input.prompt, "hello");
        assert!(input.target_agent_id.is_none());
        assert!(input.history.is_none());
    }

    #[test]
    fn test_task_input_with_agent() {
        let input = TaskInput::new("hello").with_agent("worker");
        assert_eq!(input.target_agent_id, Some("worker".into()));
    }

    #[test]
    fn test_task_input_convert_to_agent_task() {
        let input = TaskInput::new("test prompt");
        let task = input.convert_to_agent_task(TaskSource::User);
        assert_eq!(task.prompt, "test prompt");
        assert_eq!(task.target_agent_id, "main");
        assert!(matches!(task.source, TaskSource::User));
    }

    #[test]
    fn test_orchd_config_default() {
        let config = OrchdConfig::default();
        assert!(config.providers.is_empty());
        assert!(config.agents.is_empty());
        assert_eq!(config.default_model.provider, "openai");
    }

    #[test]
    fn test_orchd_config_single_provider() {
        let config = OrchdConfig::single_provider("anthropic", "sk-key", "claude-sonnet");
        assert_eq!(config.providers.len(), 1);
        assert!(config.providers.contains_key("anthropic"));
        assert_eq!(config.agents.len(), 1);
        assert!(config.agents.contains_key("main"));
    }

    #[test]
    fn test_provider_config_serde() {
        let pc = ProviderConfig {
            kind: "openai".into(),
            api_key: "sk-123".into(),
            base_url: Some("https://custom.api/v1".into()),
            headers: None,
        };
        let json = serde_json::to_string(&pc).unwrap();
        let back: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, "openai");
        assert_eq!(back.api_key, "sk-123");
        assert_eq!(back.base_url, Some("https://custom.api/v1".into()));
    }

    #[test]
    fn test_user_response_serde() {
        let ur = UserResponse::AskUser {
            answer: "yes".into(),
        };
        let json = serde_json::to_string(&ur).unwrap();
        assert!(json.contains("ask_user"));
        assert!(json.contains("yes"));
    }
}
