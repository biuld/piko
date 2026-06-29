//! BottomBar configuration types.

use serde::{Deserialize, Serialize};

/// Which items to show, in display order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BottomBarConfig {
    #[serde(default = "default_items")]
    pub items: Vec<BottomBarItem>,
}

impl Default for BottomBarConfig {
    fn default() -> Self {
        Self {
            items: default_items(),
        }
    }
}

fn default_items() -> Vec<BottomBarItem> {
    use BottomBarItem::*;
    vec![Model, Cwd, Context, Cost]
}

// ── BottomBarItem ────────────────────────────────────────────────────────────

/// Each item in the BottomBar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BottomBarItem {
    /// Model provider + model ID + thinking level.
    Model,
    /// Project working directory (abbreviated).
    Cwd,
    /// Context window usage: used / total tokens.
    Context,
    /// Cumulative session cost in USD.
    Cost,
}
