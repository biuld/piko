//! GUI-owned presentation settings persisted by hostd under `[gui]`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct GuiSettings {
    #[serde(default = "default_session_width")]
    pub session_width: f32,
    #[serde(default = "default_inspector_width")]
    pub inspector_width: f32,
    #[serde(default = "default_true")]
    pub session_open: bool,
    #[serde(default = "default_true")]
    pub inspector_open: bool,
    #[serde(default)]
    pub reduced_motion: bool,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            session_width: default_session_width(),
            inspector_width: default_inspector_width(),
            session_open: true,
            inspector_open: true,
            reduced_motion: false,
        }
    }
}

fn default_session_width() -> f32 {
    crate::app::layout_state::SESSION_DEFAULT_WIDTH
}

fn default_inspector_width() -> f32 {
    crate::app::layout_state::INSPECTOR_DEFAULT_WIDTH
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
}
