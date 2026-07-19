//! Host runtime settings snapshot (shared product config, not `[gui]`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct HostRuntimeSettings {
    #[serde(default)]
    pub compaction: CompactionSettings,
    #[serde(default)]
    pub retry: RetrySettings,
    #[serde(default)]
    pub sandbox: SandboxSettings,
    /// When `None`, all discovered tools are enabled.
    pub active_tool_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct CompactionSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct RetrySettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct SandboxSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            enabled: default_true(),
        }
    }
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            enabled: default_true(),
        }
    }
}

impl Default for SandboxSettings {
    fn default() -> Self {
        Self {
            enabled: default_true(),
        }
    }
}

fn default_true() -> bool {
    true
}

impl HostRuntimeSettings {
    /// Whether the active-tools list restricts execution (empty list = none).
    pub fn tools_restricted(&self) -> bool {
        self.active_tool_names
            .as_ref()
            .is_some_and(|names| names.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_nested_fields_use_runtime_defaults() {
        let settings: HostRuntimeSettings = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(settings.compaction.enabled);
        assert!(settings.retry.enabled);
        assert!(settings.sandbox.enabled);
        assert!(settings.active_tool_names.is_none());
    }

    #[test]
    fn reads_disabled_flags_from_host_blob() {
        let settings: HostRuntimeSettings = serde_json::from_value(serde_json::json!({
            "compaction": { "enabled": false },
            "retry": { "enabled": false },
            "sandbox": { "enabled": false },
            "active-tool-names": []
        }))
        .unwrap();
        assert!(!settings.compaction.enabled);
        assert!(!settings.retry.enabled);
        assert!(!settings.sandbox.enabled);
        assert!(settings.tools_restricted());
    }
}
