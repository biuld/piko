//! GUI-owned presentation settings persisted by hostd under `[gui]`,
//! plus typed views of shared host runtime settings.

mod host;

pub use host::HostRuntimeSettings;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct GuiSettings {
    #[serde(default = "default_session_width")]
    pub session_width: f32,
    #[serde(
        default = "default_right_column_width",
        alias = "agents-tree-width",
        alias = "agents_tree_width",
        alias = "inspector-width",
        alias = "inspector_width"
    )]
    pub right_column_width: f32,
    #[serde(default = "default_true")]
    pub session_open: bool,
    #[serde(
        default = "default_true",
        alias = "agents-tree-open",
        alias = "agents_tree_open",
        alias = "inspector-open",
        alias = "inspector_open"
    )]
    pub right_column_open: bool,
    #[serde(default)]
    pub reduced_motion: bool,
    /// GUI-only: hide thinking/reasoning blocks in the timeline. Independent
    /// of the TUI's `[tui].hide_thinking_block` — see
    /// docs/settings-ownership-design.md.
    #[serde(default)]
    pub hide_thinking_block: bool,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            session_width: default_session_width(),
            right_column_width: default_right_column_width(),
            session_open: true,
            right_column_open: true,
            reduced_motion: false,
            hide_thinking_block: false,
        }
    }
}

fn default_session_width() -> f32 {
    crate::app::layout_state::SESSION_DEFAULT_WIDTH
}

fn default_right_column_width() -> f32 {
    crate::app::layout_state::RIGHT_COLUMN_DEFAULT_WIDTH
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_use_stable_defaults() {
        let settings: GuiSettings = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(settings, GuiSettings::default());
    }

    #[test]
    fn reads_legacy_inspector_keys() {
        let settings: GuiSettings = serde_json::from_value(serde_json::json!({
            "inspector-width": 360.0,
            "inspector-open": false,
        }))
        .unwrap();
        assert_eq!(settings.right_column_width, 360.0);
        assert!(!settings.right_column_open);
    }

    #[test]
    fn reads_legacy_agents_tree_keys() {
        let settings: GuiSettings = serde_json::from_value(serde_json::json!({
            "agents-tree-width": 380.0,
            "agents-tree-open": false,
        }))
        .unwrap();
        assert_eq!(settings.right_column_width, 380.0);
        assert!(!settings.right_column_open);
    }
}
