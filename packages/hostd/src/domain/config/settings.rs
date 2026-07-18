use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

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

    // ---- Transport / TUI ----
    pub transport: Option<String>,
    pub theme: Option<String>,
    pub hide_thinking_block: Option<bool>,

    // ---- Execution ----
    pub compaction: Option<CompactionSettings>,
    pub retry: Option<RetrySettings>,
    pub sandbox: Option<SandboxSettings>,

    // ---- Paths ----
    pub session_dir: Option<String>,

    // ---- Tools ----
    /// Active tool names to enable for agent runs. When None, all tools are enabled.
    pub active_tool_names: Option<Vec<String>>,

    // ---- MCP ----
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServerConfig>,

    // ---- TUI ----
    /// TUI-specific settings stored as an opaque JSON value.
    /// The TUI owns the schema; hostd just stores and forwards it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tui: Option<serde_json::Value>,
    /// GUI-specific settings stored as an opaque JSON value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gui: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CompactionSettings {
    pub enabled: Option<bool>,
    pub reserve_tokens: Option<u64>,
    pub keep_recent_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct RetrySettings {
    pub enabled: Option<bool>,
    pub max_retries: Option<u32>,
    pub base_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct SandboxSettings {
    pub enabled: Option<bool>,
    pub policy_path: Option<String>,
    /// Path to the shell binary for command execution (default: "bash").
    pub shell_path: Option<String>,
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

    pub fn get_theme(&self) -> Option<&str> {
        self.merged.theme.as_deref()
    }

    pub fn get_compaction_settings(&self) -> (bool, u64, u64) {
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
        }),
        retry: Some(RetrySettings {
            enabled: Some(true),
            max_retries: Some(3),
            base_delay_ms: Some(2000),
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
        theme: overrides.theme.or(base.theme),
        hide_thinking_block: overrides.hide_thinking_block.or(base.hide_thinking_block),
        compaction: merge_compaction(base.compaction, overrides.compaction),
        retry: merge_retry(base.retry, overrides.retry),
        sandbox: merge_sandbox(base.sandbox, overrides.sandbox),
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
