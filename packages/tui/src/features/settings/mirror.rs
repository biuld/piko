//! Host + presentation settings mirror for the Settings surface.
//!
//! Loaded from `ConfigGet { namespace: "host" }` (and presentation fields from
//! TUI / model chrome). Optimistic apply keeps reopen correct when hostd does
//! not push a host ConfigEntry.

use serde_json::Value;

/// Defaults match hostd resources/settings.default.toml where applicable.
#[derive(Clone, Debug)]
pub struct HostRuntimeSettings {
    /// True after at least one successful host ConfigEntry apply.
    pub loaded: bool,
    pub thinking_level: Option<String>,
    pub compaction_enabled: bool,
    pub compaction_reserve: u64,
    pub compaction_keep: u64,
    pub retry_enabled: bool,
    pub sandbox_enabled: bool,
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
            retry_enabled: true,
            sandbox_enabled: false,
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
        }
        if let Some(r) = value.get("retry")
            && let Some(enabled) = r.get("enabled").and_then(|v| v.as_bool())
        {
            self.retry_enabled = enabled;
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

/// Snapshot used to build the Settings catalog and active markers.
#[derive(Clone, Debug)]
pub struct SettingsSnapshot {
    pub host: HostRuntimeSettings,
    pub thinking_level: Option<String>,
    pub thinking_visible: bool,
    pub tools_expanded: bool,
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

pub fn otel_endpoint_preset_active(host: &HostRuntimeSettings, preset: &str) -> bool {
    host.otel_endpoint == preset
}

pub fn otel_endpoint_is_custom(host: &HostRuntimeSettings, presets: &[&str]) -> bool {
    !presets.contains(&host.otel_endpoint.as_str())
}
