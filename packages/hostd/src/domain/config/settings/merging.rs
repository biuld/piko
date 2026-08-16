use super::*;

pub(crate) fn default_settings() -> HostSettings {
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

pub(crate) fn merge(base: HostSettings, overrides: HostSettings) -> HostSettings {
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
        permissions: merge_permissions(base.permissions, overrides.permissions),
        features: merge_features(base.features, overrides.features),
        execution: merge_execution(base.execution, overrides.execution),
        observability: merge_observability(base.observability, overrides.observability),
        trajectory: overrides.trajectory.or(base.trajectory),
        session_dir: overrides.session_dir.or(base.session_dir),
        active_tool_names: overrides.active_tool_names.or(base.active_tool_names),
        mcp_servers: if overrides.mcp_servers.is_empty() {
            base.mcp_servers
        } else {
            overrides.mcp_servers
        },
        mcp: overrides.mcp.or(base.mcp),
        prompt: overrides.prompt.or(base.prompt),
        tui: overrides.tui.or(base.tui),
    }
}

pub(crate) fn merge_compaction(
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

pub(crate) fn merge_transcript(
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

pub(crate) fn merge_retry(
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

pub(crate) fn merge_approvals(
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

pub(crate) fn merge_guardian(
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

pub(crate) fn merge_safety(
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

pub(crate) fn merge_permissions(
    base: Option<PermissionsSettings>,
    overrides: Option<PermissionsSettings>,
) -> Option<PermissionsSettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => {
            let mut profiles = base.profiles;
            for (name, profile) in overrides.profiles {
                profiles.insert(name, profile);
            }
            let mut roles = base.roles;
            for (role, profile) in overrides.roles {
                roles.insert(role, profile);
            }
            Some(PermissionsSettings {
                profile: overrides.profile.or(base.profile),
                profiles,
                roles,
            })
        }
        (base, overrides) => overrides.or(base),
    }
}

pub(crate) fn merge_features(
    base: Option<FeaturesSettings>,
    overrides: Option<FeaturesSettings>,
) -> Option<FeaturesSettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => {
            let mut enabled = base.enabled;
            for (key, value) in overrides.enabled {
                enabled.insert(key, value);
            }
            let mut managed = base.managed;
            for (key, value) in overrides.managed {
                managed.insert(key, value);
            }
            Some(FeaturesSettings { enabled, managed })
        }
        (base, overrides) => overrides.or(base),
    }
}

pub(crate) fn merge_execution(
    base: Option<ExecutionSettings>,
    overrides: Option<ExecutionSettings>,
) -> Option<ExecutionSettings> {
    match (base, overrides) {
        (Some(base), Some(overrides)) => Some(ExecutionSettings {
            shell: overrides.shell.or(base.shell),
        }),
        (base, overrides) => overrides.or(base),
    }
}

pub(crate) fn merge_observability(
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

pub(crate) fn load_from_file(path: &Path) -> Result<HostSettings, SettingsError> {
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

pub(crate) fn piko_dir() -> PathBuf {
    if let Some(root) = std::env::var_os("PIKO_HOME") {
        return PathBuf::from(root);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".piko")
}

#[cfg(test)]
pub(crate) fn installed_settings_fixture() -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/settings.toml"))
        .expect("installed settings fixture must be readable")
}
