use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Host-projected `[tui.keybindings]` settings. The host merges this object
/// by rule ID; the TUI performs semantic validation against its catalogs.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct KeybindingSettings {
    #[serde(default)]
    pub rules: BTreeMap<String, BindingRuleSetting>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct BindingRuleSetting {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub when: Option<Vec<String>>,
}
