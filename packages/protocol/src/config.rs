// ---- Protocol: config — orchd startup configuration types ----
//
// Host provides this once at startup. orchd uses it to wire
// providers, agents, tool sets, and runtime limits.
//
// orchd knows nothing about env vars, keychains, sessions, or users.
// All external-world knowledge comes from this config.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::agents::AgentSpec;
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

    /// Maximum provider input+output context for deterministic preflight.
    #[serde(default = "default_context_window", rename = "contextWindow")]
    pub context_window: u64,

    /// Output tokens reserved before selecting transcript context.
    #[serde(default = "default_max_output_tokens", rename = "maxOutputTokens")]
    pub max_output_tokens: u64,
}

fn default_context_window() -> u64 {
    128_000
}

fn default_max_output_tokens() -> u64 {
    4_096
}

// ---- Full startup config ----

/// Sandbox configuration passed from Host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SandboxConfig {
    /// Whether the sandbox is enabled. When false, use permissive defaults.
    #[serde(default)]
    pub enabled: bool,
    /// Path to the sandbox policy file (JSON). Relative to cwd.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_path: Option<String>,
    /// Path to the shell binary for command execution (default: "bash").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_path: Option<String>,
}

/// Retry configuration for model calls.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RetryConfig {
    /// Whether retries are enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Maximum number of retry attempts.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Base delay between retries in milliseconds.
    #[serde(default = "default_base_delay_ms")]
    pub base_delay_ms: u64,
}

fn default_true() -> bool {
    true
}
fn default_max_retries() -> u32 {
    3
}
fn default_base_delay_ms() -> u64 {
    2000
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: 3,
            base_delay_ms: 2000,
        }
    }
}

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

    /// Per-model thinking level mapping (from model catalog).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "thinkingLevelMap"
    )]
    pub thinking_level_map: super::model::ThinkingLevelMap,

    /// Sandbox policy configuration.
    #[serde(default)]
    pub sandbox: SandboxConfig,
}

impl Default for OrchdConfig {
    fn default() -> Self {
        Self {
            providers: HashMap::new(),
            agents: HashMap::new(),
            default_model: ModelRef {
                provider: "openai".into(),
                model_id: "gpt-4o".into(),
                context_window: default_context_window(),
                max_output_tokens: default_max_output_tokens(),
            },
            default_settings: ModelRunSettings::default(),
            runtime: OrchestratorRuntimeConfig::default(),
            thinking_level_map: None,
            sandbox: SandboxConfig::default(),
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
            version: "1".into(),
            provenance: crate::PromptSource::new("built-in", "agents/main"),
            name: "Main".into(),
            role: "assistant".into(),
            description: Some("Default agent".into()),
            base_instructions: String::new(),
            model: Some(model_id_str.clone()),
            tool_set_ids: vec![
                "todo".into(),
                "workspace".into(),
                "user_interaction".into(),
                "multi_agent".into(),
            ],
            active_tool_names: None,
            thinking_level: None,
        };
        let mut agents = HashMap::new();
        agents.insert("main".into(), main_agent);

        Self {
            providers,
            agents,
            default_model: ModelRef {
                provider: kind_str,
                model_id: model_id_str,
                context_window: default_context_window(),
                max_output_tokens: default_max_output_tokens(),
            },
            default_settings: ModelRunSettings::default(),
            runtime: OrchestratorRuntimeConfig::default(),
            thinking_level_map: None,
            sandbox: SandboxConfig::default(),
        }
    }
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
