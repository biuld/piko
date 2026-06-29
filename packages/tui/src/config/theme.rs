//! Theme configuration types.

use serde::{Deserialize, Serialize};

/// Theme-related TUI settings, namespaced under `tui.theme.*` on hostd.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Active theme name. "dark" or "light" for built-in, or the `name` field
    /// from a custom `.toml` file in `~/.piko/themes/` or `.piko/themes/`.
    #[serde(default = "default_theme_name")]
    pub name: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
        }
    }
}

fn default_theme_name() -> String {
    "dark".to_string()
}
