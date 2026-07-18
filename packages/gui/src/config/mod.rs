//! GUI-owned presentation settings persisted by hostd under `[gui]`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct GuiSettings {
    #[serde(default = "default_session_width")]
    pub session_width: f32,
    #[serde(
        default = "default_agents_tree_width",
        alias = "right-column-width",
        alias = "right_column_width",
        alias = "inspector-width",
        alias = "inspector_width"
    )]
    pub agents_tree_width: f32,
    #[serde(default = "default_true")]
    pub session_open: bool,
    #[serde(
        default = "default_true",
        alias = "right-column-open",
        alias = "right_column_open",
        alias = "inspector-open",
        alias = "inspector_open"
    )]
    pub agents_tree_open: bool,
    #[serde(default)]
    pub reduced_motion: bool,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            session_width: default_session_width(),
            agents_tree_width: default_agents_tree_width(),
            session_open: true,
            agents_tree_open: true,
            reduced_motion: false,
        }
    }
}

fn default_session_width() -> f32 {
    crate::app::layout_state::SESSION_DEFAULT_WIDTH
}

fn default_agents_tree_width() -> f32 {
    crate::app::layout_state::AGENTS_TREE_DEFAULT_WIDTH
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
        assert_eq!(settings.agents_tree_width, 360.0);
        assert!(!settings.agents_tree_open);
    }
}
