use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostSettings {
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    pub default_thinking_level: Option<String>,
    pub transport: Option<String>,
    pub theme: Option<String>,
    pub compaction: Option<CompactionSettings>,
    pub branch_summary: Option<BranchSummarySettings>,
    pub retry: Option<RetrySettings>,
    pub hide_thinking_block: Option<bool>,
    pub shell_path: Option<String>,
    pub sandbox: Option<SandboxSettings>,
    pub session_dir: Option<String>,
    pub double_escape_action: Option<String>,
    pub quiet_startup: Option<bool>,
    pub clear_on_shrink: Option<bool>,
    pub steering_mode: Option<String>,
    pub follow_up_mode: Option<String>,
    pub extensions: Option<Vec<String>>,
    pub skills: Option<Vec<String>>,
    pub prompts: Option<Vec<String>>,
    pub themes: Option<Vec<String>>,
    pub enabled_models: Option<Vec<String>>,
    pub agent_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompactionSettings {
    pub enabled: Option<bool>,
    pub reserve_tokens: Option<u64>,
    pub keep_recent_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BranchSummarySettings {
    pub reserve_tokens: Option<u64>,
    pub skip_prompt: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RetrySettings {
    pub enabled: Option<bool>,
    pub max_retries: Option<u32>,
    pub base_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SandboxSettings {
    pub enabled: Option<bool>,
    pub policy_path: Option<String>,
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
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
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
        let global_path = piko_dir().join("settings.json");
        let project_path = cwd.as_ref().join(".piko").join("settings.json");
        Self::from_paths(global_path, project_path, overrides)
    }

    pub fn from_paths(
        global_path: impl Into<PathBuf>,
        project_path: impl Into<PathBuf>,
        overrides: HostSettings,
    ) -> Result<Self, SettingsError> {
        let global_path = global_path.into();
        let project_path = project_path.into();
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
}

fn default_settings() -> HostSettings {
    HostSettings {
        compaction: Some(CompactionSettings {
            enabled: Some(true),
            reserve_tokens: Some(16384),
            keep_recent_tokens: Some(20000),
        }),
        branch_summary: Some(BranchSummarySettings {
            reserve_tokens: Some(16384),
            skip_prompt: Some(false),
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
        compaction: merge_compaction(base.compaction, overrides.compaction),
        branch_summary: merge_branch_summary(base.branch_summary, overrides.branch_summary),
        retry: merge_retry(base.retry, overrides.retry),
        hide_thinking_block: overrides.hide_thinking_block.or(base.hide_thinking_block),
        shell_path: overrides.shell_path.or(base.shell_path),
        sandbox: merge_sandbox(base.sandbox, overrides.sandbox),
        session_dir: overrides.session_dir.or(base.session_dir),
        double_escape_action: overrides.double_escape_action.or(base.double_escape_action),
        quiet_startup: overrides.quiet_startup.or(base.quiet_startup),
        clear_on_shrink: overrides.clear_on_shrink.or(base.clear_on_shrink),
        steering_mode: overrides.steering_mode.or(base.steering_mode),
        follow_up_mode: overrides.follow_up_mode.or(base.follow_up_mode),
        extensions: overrides.extensions.or(base.extensions),
        skills: overrides.skills.or(base.skills),
        prompts: overrides.prompts.or(base.prompts),
        themes: overrides.themes.or(base.themes),
        enabled_models: overrides.enabled_models.or(base.enabled_models),
        agent_names: overrides.agent_names.or(base.agent_names),
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

fn merge_branch_summary(
    base: Option<BranchSummarySettings>,
    overrides: Option<BranchSummarySettings>,
) -> Option<BranchSummarySettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(BranchSummarySettings {
            reserve_tokens: overrides.reserve_tokens.or(base.reserve_tokens),
            skip_prompt: overrides.skip_prompt.or(base.skip_prompt),
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
    serde_json::from_str(&content).map_err(|source| SettingsError::Json {
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
