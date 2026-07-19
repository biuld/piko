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
pub mod editor;
pub mod theme;

// ── TuiConfig ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TreeFilterMode {
    #[default]
    Default,
    NoTools,
    UserOnly,
    LabeledOnly,
    All,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TreeConfig {
    #[serde(default)]
    pub filter_mode: TreeFilterMode,
}

/// Root TUI configuration, namespaced under `tui.*` in hostd settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default)]
    pub bottom_bar: bottom_bar::BottomBarConfig,
    #[serde(default)]
    pub editor: editor::EditorConfig,
    #[serde(default)]
    pub theme: theme::ThemeConfig,
    #[serde(default)]
    pub tree: TreeConfig,
    /// TUI-only: hide thinking/reasoning blocks in the timeline. Independent
    /// of the GUI's `[gui].hide-thinking-block` — see
    /// docs/settings-ownership-design.md.
    #[serde(default)]
    pub hide_thinking_block: bool,
}

impl TuiConfig {
    /// Build from hostd-provided settings JSON value. Unknown or missing keys
    /// fall back to defaults.
    pub fn from_hostd_settings(raw: Option<&serde_json::Value>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };
        serde_json::from_value(raw.clone()).unwrap_or_default()
    }
}
