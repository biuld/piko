use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorConfig {
    #[serde(default = "default_multiline")]
    pub multiline: bool,
    #[serde(default = "default_max_lines")]
    pub max_lines: u16,
    #[serde(default)]
    pub auto_resize: bool,
    #[serde(default = "default_large_paste_lines")]
    pub large_paste_lines: usize,
    #[serde(default = "default_large_paste_chars")]
    pub large_paste_chars: usize,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            multiline: default_multiline(),
            max_lines: default_max_lines(),
            auto_resize: true,
            large_paste_lines: default_large_paste_lines(),
            large_paste_chars: default_large_paste_chars(),
            history_limit: default_history_limit(),
        }
    }
}

fn default_multiline() -> bool {
    true
}

fn default_max_lines() -> u16 {
    6
}

fn default_large_paste_lines() -> usize {
    10
}

fn default_large_paste_chars() -> usize {
    1000
}

fn default_history_limit() -> usize {
    100
}
