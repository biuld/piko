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
    pub sandbox: Option<SandboxSettings>,

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

    // ---- Frontend namespaces (opaque to hostd) ----
    /// TUI-specific settings. The TUI owns the schema; hostd stores and forwards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tui: Option<serde_json::Value>,
    /// GUI-specific settings. The GUI owns the schema; hostd stores and forwards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gui: Option<serde_json::Value>,
}

impl HostSettings {
    /// Resolve the JSON blob for a `ConfigGet` namespace.
    /// Unknown namespaces return an empty object.
    pub fn namespace_value(&self, namespace: &str) -> serde_json::Value {
        match namespace {
            "tui" => self.tui.clone().unwrap_or_else(|| serde_json::json!({})),
            "gui" => self.gui.clone().unwrap_or_else(|| serde_json::json!({})),
            "host" => self.host_namespace_value(),
            _ => serde_json::Value::Object(Default::default()),
        }
    }

    /// Shared runtime fields for `ConfigGet { namespace: "host" }`.
    /// Excludes frontend blobs (`[tui]`, `[gui]`).
    pub fn host_namespace_value(&self) -> serde_json::Value {
        serde_json::json!({
            "default-provider": self.default_provider,
            "default-model": self.default_model,
            "default-thinking-level": self.default_thinking_level,
            "compaction": self.compaction,
            "transcript": self.transcript,
            "retry": self.retry,
            "approvals": self.approvals,
            "guardian": self.guardian,
            "safety": self.safety,
            "sandbox": self.sandbox,
            "observability": self.observability,
            "active-tool-names": self.active_tool_names,
            "session-dir": self.session_dir,
            "mcp-servers": self.mcp_servers,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct SandboxSettings {
    pub enabled: Option<bool>,
    pub policy_path: Option<String>,
    /// Path to the shell binary for command execution (default: "bash").
    pub shell_path: Option<String>,
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

#[derive(Debug, Clone)]
pub struct SettingsManager {
    global_path: PathBuf,
    project_path: PathBuf,
    global_settings: HostSettings,
    project_settings: HostSettings,
    overrides: HostSettings,
    merged: HostSettings,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("failed to read settings {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse settings {path}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to serialize settings {path}: {source}")]
    TomlSerialize {
        path: PathBuf,
        #[source]
        source: toml::ser::Error,
    },
}

impl SettingsManager {
    pub fn create(cwd: impl AsRef<Path>) -> Result<Self, SettingsError> {
        Self::create_with_overrides(cwd, HostSettings::default())
    }

    pub fn create_with_overrides(
        cwd: impl AsRef<Path>,
        overrides: HostSettings,
    ) -> Result<Self, SettingsError> {
        let global_path = piko_dir().join("settings.toml");
        let project_path = cwd.as_ref().join(".piko").join("settings.toml");
        Self::from_paths(global_path, project_path, overrides)
    }

    pub fn from_paths(
        global_path: impl Into<PathBuf>,
        project_path: impl Into<PathBuf>,
        overrides: HostSettings,
    ) -> Result<Self, SettingsError> {
        let global_path = global_path.into();
        let project_path = project_path.into();

        if !global_path.exists() && !global_path.as_os_str().is_empty() {
            if let Some(parent) = global_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&global_path, default_settings_template());
        }

        let global_settings = load_from_file(&global_path)?;
        let project_settings = load_from_file(&project_path)?;
        let merged = merge(
            merge(
                merge(default_settings(), global_settings.clone()),
                project_settings.clone(),
            ),
            overrides.clone(),
        );
        Ok(Self {
            global_path,
            project_path,
            global_settings,
            project_settings,
            overrides,
            merged,
        })
    }

    pub fn in_memory(settings: HostSettings) -> Self {
        let merged = merge(default_settings(), settings.clone());
        Self {
            global_path: PathBuf::new(),
            project_path: PathBuf::new(),
            global_settings: HostSettings::default(),
            project_settings: HostSettings::default(),
            overrides: settings,
            merged,
        }
    }

    pub fn reload(&mut self) -> Result<(), SettingsError> {
        self.global_settings = load_from_file(&self.global_path)?;
        self.project_settings = load_from_file(&self.project_path)?;
        self.merged = merge(
            merge(
                merge(default_settings(), self.global_settings.clone()),
                self.project_settings.clone(),
            ),
            self.overrides.clone(),
        );
        Ok(())
    }

    pub fn apply_overrides(&mut self, overrides: HostSettings) {
        self.overrides = merge(self.overrides.clone(), overrides.clone());
        self.merged = merge(self.merged.clone(), overrides);
    }

    pub fn settings(&self) -> HostSettings {
        self.merged.clone()
    }

    pub fn get_default_provider(&self) -> Option<&str> {
        self.merged.default_provider.as_deref()
    }

    pub fn get_default_model(&self) -> Option<&str> {
        self.merged.default_model.as_deref()
    }

    pub fn get_transport(&self) -> &str {
        self.merged.transport.as_deref().unwrap_or("auto")
    }

    pub fn global_path(&self) -> &Path {
        &self.global_path
    }

    pub fn project_path(&self) -> &Path {
        &self.project_path
    }

    pub fn get_compaction_settings(&self) -> (bool, u64, u64, u64) {
        let compaction = self.merged.compaction.as_ref();
        (
            compaction
                .and_then(|settings| settings.enabled)
                .unwrap_or(true),
            compaction
                .and_then(|settings| settings.reserve_tokens)
                .unwrap_or(16384),
            compaction
                .and_then(|settings| settings.keep_recent_tokens)
                .unwrap_or(20000),
            compaction
                .and_then(|settings| settings.min_growth_tokens)
                .unwrap_or(DEFAULT_MIN_GROWTH_TOKENS),
        )
    }

    /// Apply a partial update and persist to the project settings file.
    pub fn update_and_persist(&mut self, patch: HostSettings) -> Result<(), SettingsError> {
        self.project_settings = merge(self.project_settings.clone(), patch.clone());
        self.merged = merge(self.merged.clone(), patch);
        if self.project_path.as_os_str().is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.project_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content = toml::to_string_pretty(&self.project_settings).map_err(|source| {
            SettingsError::TomlSerialize {
                path: self.project_path.clone(),
                source,
            }
        })?;
        fs::write(&self.project_path, content).map_err(|source| SettingsError::Io {
            path: self.project_path.clone(),
            source,
        })
    }
}

fn default_settings() -> HostSettings {
    HostSettings {
        compaction: Some(CompactionSettings {
            enabled: Some(true),
            reserve_tokens: Some(16384),
            keep_recent_tokens: Some(20000),
            min_growth_tokens: None,
            min_growth_fraction: Some(DEFAULT_MIN_GROWTH_FRACTION),
            summarizer_model: None,
            summarizer_provider: None,
        }),
        transcript: Some(TranscriptSettings {
            max_tool_output_tokens: Some(24000),
        }),
        retry: Some(RetrySettings {
            enabled: Some(true),
            max_retries: Some(3),
            base_delay_ms: Some(2000),
            max_delay_ms: Some(30_000),
            budget_ms: Some(60_000),
        }),
        approvals: Some(ApprovalSettings {
            timeout_secs: Some(120),
        }),
        guardian: Some(GuardianSettings {
            enabled: Some(false),
            model: None,
            provider: None,
            timeout_secs: Some(30),
            max_consecutive_denials: Some(3),
        }),
        safety: Some(SafetySettings {
            auto_approve_workspace_writes: Some(true),
        }),
        ..HostSettings::default()
    }
}

fn merge(base: HostSettings, overrides: HostSettings) -> HostSettings {
    HostSettings {
        default_provider: overrides.default_provider.or(base.default_provider),
        default_model: overrides.default_model.or(base.default_model),
        default_thinking_level: overrides
            .default_thinking_level
            .or(base.default_thinking_level),
        transport: overrides.transport.or(base.transport),
        compaction: merge_compaction(base.compaction, overrides.compaction),
        transcript: merge_transcript(base.transcript, overrides.transcript),
        retry: merge_retry(base.retry, overrides.retry),
        approvals: merge_approvals(base.approvals, overrides.approvals),
        guardian: merge_guardian(base.guardian, overrides.guardian),
        safety: merge_safety(base.safety, overrides.safety),
        sandbox: merge_sandbox(base.sandbox, overrides.sandbox),
        observability: merge_observability(base.observability, overrides.observability),
        session_dir: overrides.session_dir.or(base.session_dir),
        active_tool_names: overrides.active_tool_names.or(base.active_tool_names),
        mcp_servers: if overrides.mcp_servers.is_empty() {
            base.mcp_servers
        } else {
            overrides.mcp_servers
        },
        tui: overrides.tui.or(base.tui),
        gui: overrides.gui.or(base.gui),
    }
}

fn merge_compaction(
    base: Option<CompactionSettings>,
    overrides: Option<CompactionSettings>,
) -> Option<CompactionSettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(CompactionSettings {
            enabled: overrides.enabled.or(base.enabled),
            reserve_tokens: overrides.reserve_tokens.or(base.reserve_tokens),
            keep_recent_tokens: overrides.keep_recent_tokens.or(base.keep_recent_tokens),
            min_growth_tokens: overrides.min_growth_tokens.or(base.min_growth_tokens),
            min_growth_fraction: overrides.min_growth_fraction.or(base.min_growth_fraction),
            summarizer_model: overrides.summarizer_model.or(base.summarizer_model),
            summarizer_provider: overrides.summarizer_provider.or(base.summarizer_provider),
        }),
        (base, overrides) => overrides.or(base),
    }
}

fn merge_transcript(
    base: Option<TranscriptSettings>,
    overrides: Option<TranscriptSettings>,
) -> Option<TranscriptSettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(TranscriptSettings {
            max_tool_output_tokens: overrides
                .max_tool_output_tokens
                .or(base.max_tool_output_tokens),
        }),
        (base, overrides) => overrides.or(base),
    }
}

fn merge_retry(
    base: Option<RetrySettings>,
    overrides: Option<RetrySettings>,
) -> Option<RetrySettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(RetrySettings {
            enabled: overrides.enabled.or(base.enabled),
            max_retries: overrides.max_retries.or(base.max_retries),
            base_delay_ms: overrides.base_delay_ms.or(base.base_delay_ms),
            max_delay_ms: overrides.max_delay_ms.or(base.max_delay_ms),
            budget_ms: overrides.budget_ms.or(base.budget_ms),
        }),
        (base, overrides) => overrides.or(base),
    }
}

fn merge_approvals(
    base: Option<ApprovalSettings>,
    overrides: Option<ApprovalSettings>,
) -> Option<ApprovalSettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(ApprovalSettings {
            timeout_secs: overrides.timeout_secs.or(base.timeout_secs),
        }),
        (base, overrides) => overrides.or(base),
    }
}

fn merge_guardian(
    base: Option<GuardianSettings>,
    overrides: Option<GuardianSettings>,
) -> Option<GuardianSettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(GuardianSettings {
            enabled: overrides.enabled.or(base.enabled),
            model: overrides.model.or(base.model),
            provider: overrides.provider.or(base.provider),
            timeout_secs: overrides.timeout_secs.or(base.timeout_secs),
            max_consecutive_denials: overrides
                .max_consecutive_denials
                .or(base.max_consecutive_denials),
        }),
        (base, overrides) => overrides.or(base),
    }
}

fn merge_safety(
    base: Option<SafetySettings>,
    overrides: Option<SafetySettings>,
) -> Option<SafetySettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(SafetySettings {
            auto_approve_workspace_writes: overrides
                .auto_approve_workspace_writes
                .or(base.auto_approve_workspace_writes),
        }),
        (base, overrides) => overrides.or(base),
    }
}

fn merge_sandbox(
    base: Option<SandboxSettings>,
    overrides: Option<SandboxSettings>,
) -> Option<SandboxSettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(SandboxSettings {
            enabled: overrides.enabled.or(base.enabled),
            policy_path: overrides.policy_path.or(base.policy_path),
            shell_path: overrides.shell_path.or(base.shell_path),
        }),
        (base, overrides) => overrides.or(base),
    }
}

fn merge_observability(
    base: Option<ObservabilitySettings>,
    overrides: Option<ObservabilitySettings>,
) -> Option<ObservabilitySettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(ObservabilitySettings {
            enabled: overrides.enabled.or(base.enabled),
            otel_endpoint: overrides.otel_endpoint.or(base.otel_endpoint),
            service_name: overrides.service_name.or(base.service_name),
        }),
        (base, overrides) => overrides.or(base),
    }
}

fn load_from_file(path: &Path) -> Result<HostSettings, SettingsError> {
    if path.as_os_str().is_empty() || !path.exists() {
        return Ok(HostSettings::default());
    }
    let content = fs::read_to_string(path).map_err(|source| SettingsError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&content).map_err(|source| SettingsError::Toml {
        path: path.to_path_buf(),
        source,
    })
}

fn piko_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".piko")
}

fn default_settings_template() -> &'static str {
    include_str!("../../../resources/settings.default.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::guardian::GuardianConfig;

    #[test]
    fn guardian_defaults_are_documented_in_template() {
        let template = default_settings_template();
        assert!(template.contains("[guardian]"));
        assert!(template.contains("enabled = false"));
    }

    #[test]
    fn guardian_settings_merge_field_by_field() {
        let base = HostSettings {
            guardian: Some(GuardianSettings {
                enabled: Some(false),
                model: Some("base-model".into()),
                provider: None,
                timeout_secs: Some(30),
                max_consecutive_denials: Some(3),
            }),
            ..HostSettings::default()
        };
        let overrides = HostSettings {
            guardian: Some(GuardianSettings {
                enabled: Some(true),
                model: None,
                provider: Some("override-provider".into()),
                timeout_secs: None,
                max_consecutive_denials: Some(5),
            }),
            ..HostSettings::default()
        };
        let merged = merge(base, overrides);
        let guardian = merged.guardian.expect("guardian section present");
        assert_eq!(guardian.enabled, Some(true));
        assert_eq!(guardian.model.as_deref(), Some("base-model"));
        assert_eq!(guardian.provider.as_deref(), Some("override-provider"));
        assert_eq!(guardian.timeout_secs, Some(30));
        assert_eq!(guardian.max_consecutive_denials, Some(5));
    }

    #[test]
    fn guardian_config_resolves_defaults_and_disablement() {
        let settings = GuardianSettings {
            enabled: Some(true),
            model: None,
            provider: None,
            timeout_secs: None,
            max_consecutive_denials: None,
        };
        let config = GuardianConfig::from_settings(Some(&settings)).expect("enabled");
        assert!(config.enabled);
        assert_eq!(config.timeout.as_secs(), 30);
        assert_eq!(config.max_consecutive_denials, 3);

        let disabled = GuardianSettings {
            enabled: Some(false),
            ..settings
        };
        assert!(GuardianConfig::from_settings(Some(&disabled)).is_none());
        assert!(GuardianConfig::from_settings(None).is_none());
    }

    #[test]
    fn safety_defaults_are_documented_in_template() {
        let template = default_settings_template();
        assert!(template.contains("[safety]"));
        assert!(template.contains("auto-approve-workspace-writes = true"));
    }

    #[test]
    fn safety_settings_merge_field_by_field() {
        let base = HostSettings {
            safety: Some(SafetySettings {
                auto_approve_workspace_writes: Some(true),
            }),
            ..HostSettings::default()
        };
        let overrides = HostSettings {
            safety: Some(SafetySettings {
                auto_approve_workspace_writes: Some(false),
            }),
            ..HostSettings::default()
        };
        let merged = merge(base, overrides);
        assert_eq!(
            merged
                .safety
                .expect("safety section present")
                .auto_approve_workspace_writes,
            Some(false)
        );

        // Missing override inherits the base value.
        let merged_inherit = merge(
            HostSettings {
                safety: Some(SafetySettings {
                    auto_approve_workspace_writes: Some(true),
                }),
                ..HostSettings::default()
            },
            HostSettings::default(),
        );
        assert_eq!(
            merged_inherit
                .safety
                .expect("safety section present")
                .auto_approve_workspace_writes,
            Some(true)
        );
    }
}
