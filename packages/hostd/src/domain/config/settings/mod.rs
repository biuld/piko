use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::domain::compaction::{DEFAULT_MIN_GROWTH_FRACTION, DEFAULT_MIN_GROWTH_TOKENS};

/// Configuration for an MCP (Model Context Protocol) server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// F-13: per-server connect timeout override (ms). Wins over
    /// `[mcp] connect-timeout-ms`; the default is 10 s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// F-13: `[mcp]` settings section.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct McpSettings {
    /// Connect/prewarm timeout per MCP server in milliseconds (default 10000).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connect_timeout_ms: Option<u64>,
    /// Operator-authored approval prompts keyed by `"server/tool"` or bare
    /// `"tool"`. Rendered into `ApprovalSnapshot.prompt` when an MCP tool
    /// needs approval; presentation text only, never policy.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub approval_templates: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct HostSettings {
    // ---- Model ----
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub default_thinking_level: Option<piko_protocol::model::ThinkingLevel>,

    // ---- Transport ----
    pub transport: Option<String>,

    // ---- Execution ----
    pub compaction: Option<CompactionSettings>,
    pub transcript: Option<TranscriptSettings>,
    pub retry: Option<RetrySettings>,
    pub approvals: Option<ApprovalSettings>,
    pub guardian: Option<GuardianSettings>,
    pub safety: Option<SafetySettings>,
    pub permissions: Option<PermissionsSettings>,
    pub features: Option<FeaturesSettings>,
    pub execution: Option<ExecutionSettings>,

    // ---- Observability ----
    pub observability: Option<ObservabilitySettings>,

    // ---- Paths ----
    pub session_dir: Option<String>,

    // ---- Tools ----
    /// Active tool names to enable for agent runs. When None, all tools are enabled.
    pub active_tool_names: Option<Vec<String>>,

    // ---- MCP ----
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServerConfig>,
    /// F-13: `[mcp]` section (connect timeout, approval templates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpSettings>,

    /// F-03/D-28: prompt assembly / provider cache policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<PromptSettings>,

    // ---- Frontend namespaces (opaque to hostd) ----
    /// TUI-specific settings. The TUI owns the schema; hostd stores and forwards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tui: Option<serde_json::Value>,
}

/// F-03/D-28: `[prompt]` settings section.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PromptSettings {
    /// Provider prompt-cache policy for assembled agent runs.
    /// Values: `disabled`, `provider-default`, `ephemeral`, `extended`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_policy: Option<PromptCachePolicySetting>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PromptCachePolicySetting {
    Disabled,
    #[default]
    ProviderDefault,
    Ephemeral,
    Extended,
}

impl PromptCachePolicySetting {
    pub fn to_protocol(self) -> piko_protocol::PromptCachePolicy {
        match self {
            Self::Disabled => piko_protocol::PromptCachePolicy::Disabled,
            Self::ProviderDefault => piko_protocol::PromptCachePolicy::ProviderDefault,
            Self::Ephemeral => piko_protocol::PromptCachePolicy::Ephemeral,
            Self::Extended => piko_protocol::PromptCachePolicy::Extended,
        }
    }
}

impl HostSettings {
    /// Resolve the JSON blob for a `ConfigGet` namespace.
    /// Unknown namespaces return an empty object.
    pub fn namespace_value(&self, namespace: &str) -> serde_json::Value {
        match namespace {
            "tui" => self.tui.clone().unwrap_or_else(|| serde_json::json!({})),
            "host" => self.host_namespace_value(),
            _ => serde_json::Value::Object(Default::default()),
        }
    }

    /// Resolved prompt-cache policy for agent runs (F-03 / D-28).
    pub fn prompt_cache_policy(&self) -> piko_protocol::PromptCachePolicy {
        self.prompt
            .as_ref()
            .and_then(|prompt| prompt.cache_policy)
            .unwrap_or_default()
            .to_protocol()
    }

    /// Shared runtime fields for `ConfigGet { namespace: "host" }`.
    /// Excludes the frontend blob (`[tui]`).
    pub fn host_namespace_value(&self) -> serde_json::Value {
        serde_json::json!({
            "default-provider": self.default_provider,
            "default-model": self.default_model,
            "default-thinking-level": self.default_thinking_level,
            "transport": self.transport,
            "compaction": self.compaction,
            "transcript": self.transcript,
            "retry": self.retry,
            "approvals": self.approvals,
            "guardian": self.guardian,
            "safety": self.safety,
            "permissions": self.permissions,
            "features": self.features,
            "execution": self.execution,
            "observability": self.observability,
            "active-tool-names": self.active_tool_names,
            "session-dir": self.session_dir,
            "mcp-servers": self.mcp_servers,
            "mcp": self.mcp,
            "prompt": self.prompt,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CompactionSettings {
    pub enabled: Option<bool>,
    pub reserve_tokens: Option<u64>,
    pub keep_recent_tokens: Option<u64>,
    /// Hysteresis guard: minimum estimated-token growth since the last
    /// compaction before the next auto-compact may trigger. When unset,
    /// hostd derives it from the resolved model's context window using
    /// `min_growth_fraction` (slice 2), falling back to a constant when the
    /// window is unknown.
    pub min_growth_tokens: Option<u64>,
    /// Ratio of the resolved context window used as the hysteresis guard
    /// when `min_growth_tokens` is unset (F-05 slice 2). Default `0.125`.
    pub min_growth_fraction: Option<f64>,
    /// Optional model used for summarization (piko's adaptation of remote
    /// compaction). Falls back to the default model on failure.
    pub summarizer_model: Option<String>,
    pub summarizer_provider: Option<String>,
}

/// Model-view settings wired into the orchd transcript policy (F-04/F-05).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct TranscriptSettings {
    pub max_tool_output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct RetrySettings {
    pub enabled: Option<bool>,
    pub max_retries: Option<u32>,
    pub base_delay_ms: Option<u64>,
    pub max_delay_ms: Option<u64>,
    pub budget_ms: Option<u64>,
}

/// Tool-approval request behavior.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct ApprovalSettings {
    /// How long a pending approval waits for a user decision before it
    /// expires and the tool call fails closed. Default: 120 seconds.
    pub timeout_secs: Option<u64>,
}

/// Guardian auto-review behavior (F-11). When enabled, on-request tool
/// approvals are first reviewed by a bounded model call over a bounded slice
/// of the session transcript; allow executes the call once (no store grant),
/// deny fails closed, and timeout/malformed output fails closed. Consecutive
/// non-accepting outcomes trip a per-session circuit breaker that escalates
/// to the user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct GuardianSettings {
    /// Master switch for guardian auto-review. Default: false.
    pub enabled: Option<bool>,
    /// Reviewer model id. Falls back to the session default model.
    pub model: Option<String>,
    /// Reviewer provider. Falls back to the session default provider.
    pub provider: Option<String>,
    /// Review deadline in seconds before the request fails closed.
    /// Default: 30.
    pub timeout_secs: Option<u64>,
    /// Consecutive non-accepting review outcomes (denies + failures) that
    /// trip the circuit breaker and escalate to the user. Default: 3.
    pub max_consecutive_denials: Option<u32>,
}

/// Deterministic write-safety behavior (F-12). When enabled, workspace
/// write approvals (`edit` / `write`) whose targets are fully inside the
/// sandbox writable roots are auto-approved one-shot (no prompt, no store
/// grant); out-of-roots targets fail closed with `safety_rejected`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct SafetySettings {
    /// Auto-approve workspace writes whose targets are fully inside the
    /// sandbox writable roots. Default: true.
    pub auto_approve_workspace_writes: Option<bool>,
}

/// Named permission-profile selection and definitions (F-17). A profile
/// bundles file/network policy (materialized into the sandbox policy) and
/// command policy (materialized into the approval gateway): commands
/// matching an `allowed-commands` prefix execute one-shot without a prompt,
/// commands matching a `denied-commands` prefix fail closed with
/// `permission_denied`. `roles` (F-19) attaches profiles to agent roles:
/// every agent instance whose spec role is mapped executes under that
/// profile's command and file/network policy; unmapped roles inherit the
/// session `profile`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PermissionsSettings {
    /// Active profile name. Defaults to the built-in "default".
    pub profile: Option<String>,
    /// Named profile definitions; merged per name across settings layers.
    #[serde(default)]
    pub profiles: HashMap<String, PermissionProfileSettings>,
    /// F-19: role → profile-name selection; merged per key across settings
    /// layers (override wins per key, base-only keys survive). A mapping to
    /// the built-in "default" or to an unknown profile is a no-op: the role
    /// inherits the session `profile`.
    #[serde(default)]
    pub roles: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PermissionProfileSettings {
    /// Read roots for sandboxed tools. Default: the working directory.
    #[serde(default)]
    pub read_roots: Vec<String>,
    /// Write roots for sandboxed tools. Default: the working directory.
    #[serde(default)]
    pub write_roots: Vec<String>,
    /// Explicit writable scratch roots. Empty uses platform defaults.
    #[serde(default)]
    pub scratch_roots: Vec<String>,
    /// Paths denied for sandboxed tools (deny wins over roots).
    #[serde(default)]
    pub deny_paths: Vec<String>,
    /// Whether sandboxed tools may open network connections. Default: false.
    #[serde(default)]
    pub allow_network: bool,
    /// Whether an explicitly approved command may run outside containment.
    /// Defaults to true; profiles can forbid elevation completely.
    pub allow_escalation: Option<bool>,
    /// Command prefix rules that auto-accept on-request approvals one-shot
    /// (no store grant).
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    /// Command prefix rules that fail closed with `permission_denied`
    /// before any store grant, guardian review, or user prompt.
    #[serde(default)]
    pub denied_commands: Vec<String>,
}

/// Managed feature gating (F-18): named tool-family feature flags resolved
/// once per session start. `enabled` sets explicit values per key (merged
/// per key across layers, override wins per key); `managed` pins features
/// to a fixed value that is the final authority over `enabled` in every
/// layer (a conflicting explicit value logs a warning and the pin wins).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct FeaturesSettings {
    /// Explicit per-key enablement.
    #[serde(default, flatten)]
    pub enabled: HashMap<String, bool>,
    /// Operator pins; final authority over `enabled` in every layer.
    #[serde(default)]
    pub managed: HashMap<String, bool>,
}

impl<'de> Deserialize<'de> for FeaturesSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "kebab-case")]
        struct Wire {
            /// Legacy shape: `[features.enabled]`.
            #[serde(default)]
            enabled: HashMap<String, bool>,
            #[serde(default)]
            managed: HashMap<String, bool>,
            /// Current shape: booleans directly under `[features]`.
            #[serde(default, flatten)]
            flat: HashMap<String, bool>,
        }

        let Wire {
            mut enabled,
            managed,
            flat,
        } = Wire::deserialize(deserializer)?;
        enabled.extend(flat);
        Ok(Self { enabled, managed })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct ExecutionSettings {
    /// Shell used for command programs. When absent, environment discovery
    /// selects `$SHELL` and then a platform fallback.
    pub shell: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct ObservabilitySettings {
    /// Master switch for OTLP export (traces + metrics). Default: off.
    pub enabled: Option<bool>,
    /// OTLP HTTP endpoint (base URL; `/v1/traces` and `/v1/metrics` are
    /// appended automatically). Default: `http://127.0.0.1:4318`.
    pub otel_endpoint: Option<String>,
    /// OTel `service.name` resource attribute. Default: `piko-hostd`.
    pub service_name: Option<String>,
}

mod manager;
mod merging;
#[cfg(test)]
mod tests;

pub use manager::{SettingsError, SettingsManager};
