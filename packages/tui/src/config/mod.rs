//! TUI configuration.
//!
//! Settings live under the `tui` namespace on hostd. This module provides:
//! - Per-panel config types (in sub-modules)
//! - `TuiConfig` — root struct that owns all panel configs
//! - Load / update helpers that talk to hostd
//!
//! TUI does not care about the storage format (JSON, TOML, etc.) — it only
//! deals with the deserialized struct.

use serde::{Deserialize, Serialize};

pub mod bottom_bar;
pub mod theme;

// ── TuiConfig ────────────────────────────────────────────────────────────────

/// Root TUI configuration, namespaced under `tui.*` in hostd settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default)]
    pub bottom_bar: bottom_bar::BottomBarConfig,
    #[serde(default)]
    pub theme: theme::ThemeConfig,
}

impl TuiConfig {
    /// Build from hostd-provided settings JSON value. Unknown or missing keys
    /// fall back to defaults.
    #[allow(dead_code)] // wired up when hostd settings API exposes tui namespace
    pub fn from_hostd_settings(raw: Option<&serde_json::Value>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        serde_json::from_value(raw.clone()).unwrap_or_default()
    }

    /// Merge a partial update from hostd. Only the provided keys overwrite
    /// current values; everything else stays unchanged.
    #[allow(dead_code)]
    pub fn apply_update(&mut self, update: &serde_json::Value) {
        if let Ok(merged) = serde_json::from_value({
            let mut base = serde_json::to_value(&self).unwrap_or_default();
            json_merge(&mut base, update);
            base
        }) {
            *self = merged;
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Shallow merge `patch` into `base`. Both must be JSON objects.
fn json_merge(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                match (base_map.get_mut(key), value) {
                    (Some(serde_json::Value::Object(_)), serde_json::Value::Object(_)) => {
                        json_merge(base_map.get_mut(key).unwrap(), value);
                    }
                    _ => {
                        base_map.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base_val, patch_val) => {
            *base_val = patch_val.clone();
        }
    }
}
