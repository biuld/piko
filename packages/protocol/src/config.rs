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

// ---- Model reference ----

/// Lightweight reference to a model within a configured provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    /// Product/authentication provider identifier.
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
    /// Path to the shell binary for command execution (default: "bash").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_path: Option<String>,
    /// Materialized permission-profile file/network policy (F-17). Applied
    /// for the current session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_profile: Option<PermissionPolicy>,
    /// F-19: materialized per-role file/network policies keyed by agent
    /// role. A role with an entry uses this policy for workspace tools;
    /// absent roles use the session policy (`policy_profile` or the file/
    /// default resolution).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub role_policies: HashMap<String, PermissionPolicy>,
}

/// Materialized file/network policy from a permission profile (F-17).
/// Empty rule lists inherit the orchestrator's permissive defaults per
/// field, so partial profiles do not lock down access unexpectedly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPolicy {
    #[serde(default)]
    pub read_roots: Vec<String>,
    #[serde(default)]
    pub write_roots: Vec<String>,
    #[serde(default)]
    pub scratch_roots: Vec<String>,
    #[serde(default)]
    pub deny_paths: Vec<String>,
    #[serde(default)]
    pub allow_network: bool,
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
    /// Cap on a single retry delay in milliseconds.
    #[serde(default = "default_max_delay_ms")]
    pub max_delay_ms: u64,
    /// Total retry-time budget in milliseconds shared across all retries of
    /// one request (open-phase retries and mid-stream restarts combined).
    #[serde(default = "default_budget_ms")]
    pub budget_ms: u64,
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
fn default_max_delay_ms() -> u64 {
    30_000
}
fn default_budget_ms() -> u64 {
    60_000
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: 3,
            base_delay_ms: 2000,
            max_delay_ms: default_max_delay_ms(),
            budget_ms: default_budget_ms(),
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

    /// Sandbox policy configuration.
    #[serde(default)]
    pub sandbox: SandboxConfig,

    /// Max estimated tokens for a single tool result in the model view
    /// (F-04 truncation cap, wired from `[transcript]` settings in F-05).
    #[serde(
        default = "default_max_tool_output_tokens",
        rename = "transcriptMaxToolOutputTokens"
    )]
    pub transcript_max_tool_output_tokens: u64,

    /// Resolved managed-feature map (F-18): canonical feature key → bool.
    /// Absent keys are treated as enabled by orchd, so legacy/default
    /// configs are unchanged. Hostd always sends the full resolved map when
    /// a `[features]` section exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

fn default_max_tool_output_tokens() -> u64 {
    24_000
}

impl Default for OrchdConfig {
    fn default() -> Self {
        Self {
            agents: HashMap::new(),
            default_model: ModelRef {
                provider: "openai".into(),
                model_id: "gpt-4o".into(),
                context_window: default_context_window(),
                max_output_tokens: default_max_output_tokens(),
            },
            default_settings: ModelRunSettings::default(),
            runtime: OrchestratorRuntimeConfig::default(),
            sandbox: SandboxConfig::default(),
            transcript_max_tool_output_tokens: default_max_tool_output_tokens(),
            features: None,
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
        assert!(config.agents.is_empty());
        assert_eq!(config.default_model.provider, "openai");
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
