//! Host + presentation settings mirror for the Settings surface.
//!
//! Loaded from `ConfigGet { namespace: "host" }` (and presentation fields from
//! TUI / model chrome). Optimistic apply keeps reopen correct when hostd does
//! not push a host ConfigEntry.

use serde_json::Value;
use std::collections::HashMap;

/// Defaults match hostd resources/settings.default.toml where applicable.
#[derive(Clone, Debug)]
pub struct HostRuntimeSettings {
    /// True after at least one successful host ConfigEntry apply.
    pub loaded: bool,
    pub thinking_level: Option<String>,
    pub compaction_enabled: bool,
    pub compaction_reserve: u64,
    pub compaction_keep: u64,
    pub compaction_min_growth_fraction: f64,
    pub transcript_max_tool_output_tokens: u64,
    pub retry_enabled: bool,
    pub retry_max_retries: u32,
    pub retry_base_delay_ms: u64,
    pub retry_max_delay_ms: u64,
    pub retry_budget_ms: u64,
    pub approval_timeout_secs: u64,
    pub guardian_enabled: bool,
    pub guardian_timeout_secs: u64,
    pub guardian_max_consecutive_denials: u32,
    pub safe_workspace_writes: bool,
    pub permission_profile: String,
    pub permission_profiles: Vec<String>,
    pub features: HashMap<String, bool>,
    pub managed_features: HashMap<String, bool>,
    pub sandbox_enabled: bool,
    pub mcp_connect_timeout_ms: u64,
    pub prompt_cache_policy: String,
    pub observability_enabled: bool,
    pub otel_endpoint: String,
    /// `true` when active-tool-names is null/absent (all tools).
    pub all_tools: bool,
    pub transport: Option<String>,
}

impl Default for HostRuntimeSettings {
    fn default() -> Self {
        Self {
            loaded: false,
            thinking_level: None,
            compaction_enabled: true,
            compaction_reserve: 16384,
            compaction_keep: 20000,
            compaction_min_growth_fraction: 0.125,
            transcript_max_tool_output_tokens: 24000,
            retry_enabled: true,
            retry_max_retries: 3,
            retry_base_delay_ms: 2000,
            retry_max_delay_ms: 30_000,
            retry_budget_ms: 60_000,
            approval_timeout_secs: 120,
            guardian_enabled: false,
            guardian_timeout_secs: 30,
            guardian_max_consecutive_denials: 3,
            safe_workspace_writes: true,
            permission_profile: "default".to_string(),
            permission_profiles: vec!["default".to_string()],
            features: default_features(),
            managed_features: HashMap::new(),
            sandbox_enabled: false,
            mcp_connect_timeout_ms: 10_000,
            prompt_cache_policy: "provider-default".to_string(),
            observability_enabled: false,
            otel_endpoint: "http://localhost:4318".to_string(),
            all_tools: true,
            transport: None,
        }
    }
}

impl HostRuntimeSettings {
    pub fn apply_host_json(&mut self, value: &Value) {
        self.loaded = true;
        if let Some(level) = value.get("default-thinking-level").and_then(|v| v.as_str()) {
            self.thinking_level = Some(level.to_string());
        }
        if let Some(c) = value.get("compaction") {
            if let Some(enabled) = c.get("enabled").and_then(|v| v.as_bool()) {
                self.compaction_enabled = enabled;
            }
            if let Some(n) = c
                .get("reserve-tokens")
                .and_then(|v| v.as_u64())
                .or_else(|| c.get("reserve_tokens").and_then(|v| v.as_u64()))
            {
                self.compaction_reserve = n;
            }
            if let Some(n) = c
                .get("keep-recent-tokens")
                .and_then(|v| v.as_u64())
                .or_else(|| c.get("keep_recent_tokens").and_then(|v| v.as_u64()))
            {
                self.compaction_keep = n;
            }
            if let Some(n) = c
                .get("min-growth-fraction")
                .and_then(|v| v.as_f64())
                .or_else(|| c.get("min_growth_fraction").and_then(|v| v.as_f64()))
            {
                self.compaction_min_growth_fraction = n;
            }
        }
        if let Some(t) = value.get("transcript")
            && let Some(n) = kebab_u64(t, "max-tool-output-tokens", "max_tool_output_tokens")
        {
            self.transcript_max_tool_output_tokens = n;
        }
        if let Some(r) = value.get("retry") {
            if let Some(enabled) = r.get("enabled").and_then(|v| v.as_bool()) {
                self.retry_enabled = enabled;
            }
            if let Some(n) = kebab_u64(r, "max-retries", "max_retries") {
                self.retry_max_retries = n as u32;
            }
            if let Some(n) = kebab_u64(r, "base-delay-ms", "base_delay_ms") {
                self.retry_base_delay_ms = n;
            }
            if let Some(n) = kebab_u64(r, "max-delay-ms", "max_delay_ms") {
                self.retry_max_delay_ms = n;
            }
            if let Some(n) = kebab_u64(r, "budget-ms", "budget_ms") {
                self.retry_budget_ms = n;
            }
        }
        if let Some(a) = value.get("approvals")
            && let Some(n) = kebab_u64(a, "timeout-secs", "timeout_secs")
        {
            self.approval_timeout_secs = n;
        }
        if let Some(g) = value.get("guardian") {
            if let Some(enabled) = g.get("enabled").and_then(|v| v.as_bool()) {
                self.guardian_enabled = enabled;
            }
            if let Some(n) = kebab_u64(g, "timeout-secs", "timeout_secs") {
                self.guardian_timeout_secs = n;
            }
            if let Some(n) = kebab_u64(g, "max-consecutive-denials", "max_consecutive_denials") {
                self.guardian_max_consecutive_denials = n as u32;
            }
        }
        if let Some(s) = value.get("safety")
            && let Some(enabled) = s
                .get("auto-approve-workspace-writes")
                .and_then(|v| v.as_bool())
                .or_else(|| {
                    s.get("auto_approve_workspace_writes")
                        .and_then(|v| v.as_bool())
                })
        {
            self.safe_workspace_writes = enabled;
        }
        if let Some(p) = value.get("permissions") {
            if let Some(profile) = p.get("profile").and_then(|v| v.as_str()) {
                self.permission_profile = profile.to_string();
            }
            if let Some(profiles) = p.get("profiles").and_then(|v| v.as_object()) {
                self.permission_profiles = vec!["default".to_string()];
                self.permission_profiles.extend(profiles.keys().cloned());
                self.permission_profiles.sort();
                self.permission_profiles.dedup();
            }
        }
        if let Some(features) = value.get("features").and_then(|v| v.as_object()) {
            self.features = default_features();
            let enabled = features
                .get("enabled")
                .and_then(|v| v.as_object())
                .unwrap_or(features);
            for (key, value) in enabled {
                if key != "managed"
                    && let Some(enabled) = value.as_bool()
                    && self.features.contains_key(key)
                {
                    self.features.insert(key.clone(), enabled);
                }
            }
            self.managed_features = features
                .get("managed")
                .and_then(|v| v.as_object())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|(key, value)| value.as_bool().map(|v| (key.clone(), v)))
                        .collect()
                })
                .unwrap_or_default();
            for (key, value) in &self.managed_features {
                if self.features.contains_key(key) {
                    self.features.insert(key.clone(), *value);
                }
            }
        }
        if let Some(s) = value.get("sandbox")
            && let Some(enabled) = s.get("enabled").and_then(|v| v.as_bool())
        {
            self.sandbox_enabled = enabled;
        }
        if let Some(o) = value.get("observability") {
            if let Some(enabled) = o.get("enabled").and_then(|v| v.as_bool()) {
                self.observability_enabled = enabled;
            }
            if let Some(ep) = o
                .get("otel-endpoint")
                .and_then(|v| v.as_str())
                .or_else(|| o.get("otel_endpoint").and_then(|v| v.as_str()))
            {
                self.otel_endpoint = ep.to_string();
            }
        }
        if let Some(mcp) = value.get("mcp")
            && let Some(n) = kebab_u64(mcp, "connect-timeout-ms", "connect_timeout_ms")
        {
            self.mcp_connect_timeout_ms = n;
        }
        if let Some(policy) = value
            .get("prompt")
            .and_then(|v| v.get("cache-policy").or_else(|| v.get("cache_policy")))
            .and_then(|v| v.as_str())
        {
            self.prompt_cache_policy = policy.to_string();
        }
        if let Some(tools) = value.get("active-tool-names") {
            self.all_tools = tools.is_null();
            if tools.as_array().is_some_and(|a| a.is_empty()) {
                self.all_tools = false;
            }
        }
        if let Some(t) = value.get("transport").and_then(|v| v.as_str()) {
            self.transport = Some(t.to_string());
        }
    }
}

const FEATURE_KEYS: &[&str] = &[
    "workspace",
    "bash",
    "process",
    "environment",
    "context",
    "todo",
    "multi-agent",
    "user-interaction",
    "mcp",
];

fn default_features() -> HashMap<String, bool> {
    FEATURE_KEYS
        .iter()
        .map(|key| ((*key).to_string(), true))
        .collect()
}

fn kebab_u64(value: &Value, kebab: &str, snake: &str) -> Option<u64> {
    value
        .get(kebab)
        .and_then(Value::as_u64)
        .or_else(|| value.get(snake).and_then(Value::as_u64))
}

/// Snapshot used to build the Settings catalog and active markers.
#[derive(Clone, Debug)]
pub struct SettingsSnapshot {
    pub host: HostRuntimeSettings,
    pub tui: crate::config::TuiConfig,
    pub thinking_level: Option<String>,
    pub thinking_visible: bool,
    pub theme_name: String,
    pub no_tools: bool,
}

fn fmt_tokens(n: u64) -> String {
    if n >= 1024 && n.is_multiple_of(1024) {
        format!("{}k", n / 1024)
    } else if n >= 1000 && n.is_multiple_of(1000) {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

pub fn compaction_summary(host: &HostRuntimeSettings) -> String {
    if host.compaction_enabled {
        format!(
            "on · reserve {} · keep {}",
            fmt_tokens(host.compaction_reserve),
            fmt_tokens(host.compaction_keep)
        )
    } else {
        "off".to_string()
    }
}

pub fn on_off(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

pub fn observability_summary(host: &HostRuntimeSettings) -> String {
    if host.observability_enabled {
        format!("on · {}", host.otel_endpoint)
    } else {
        "off".to_string()
    }
}

pub fn guardian_summary(host: &HostRuntimeSettings) -> String {
    if host.guardian_enabled {
        format!(
            "on · {}s · trip {}",
            host.guardian_timeout_secs, host.guardian_max_consecutive_denials
        )
    } else {
        "off".to_string()
    }
}

pub fn feature_summary(host: &HostRuntimeSettings) -> String {
    let enabled = host.features.values().filter(|enabled| **enabled).count();
    let mut summary = format!("{enabled}/{} on", host.features.len());
    if !host.managed_features.is_empty() {
        summary.push_str(&format!(" · {} managed", host.managed_features.len()));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_mirror_reads_expanded_catalog_and_managed_features() {
        let mut mirror = HostRuntimeSettings::default();
        mirror.apply_host_json(&serde_json::json!({
            "transport": "stdio",
            "transcript": { "max-tool-output-tokens": 8000 },
            "retry": { "max-retries": 5, "budget-ms": 120000 },
            "approvals": { "timeout-secs": 300 },
            "guardian": {
                "enabled": true,
                "timeout-secs": 60,
                "max-consecutive-denials": 5
            },
            "permissions": {
                "profile": "locked",
                "profiles": { "locked": {} }
            },
            "features": {
                "process": true,
                "managed": { "process": false }
            },
            "mcp": { "connect-timeout-ms": 30000 },
            "prompt": { "cache-policy": "ephemeral" }
        }));

        assert_eq!(mirror.transport.as_deref(), Some("stdio"));
        assert_eq!(mirror.transcript_max_tool_output_tokens, 8000);
        assert_eq!(mirror.retry_max_retries, 5);
        assert_eq!(mirror.retry_budget_ms, 120000);
        assert_eq!(mirror.approval_timeout_secs, 300);
        assert!(mirror.guardian_enabled);
        assert_eq!(mirror.permission_profile, "locked");
        assert!(mirror.permission_profiles.contains(&"locked".to_string()));
        assert_eq!(mirror.features.get("process"), Some(&false));
        assert_eq!(mirror.managed_features.get("process"), Some(&false));
        assert_eq!(mirror.mcp_connect_timeout_ms, 30000);
        assert_eq!(mirror.prompt_cache_policy, "ephemeral");
    }
}
