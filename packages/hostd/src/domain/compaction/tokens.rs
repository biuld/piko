// ---- Domain: compaction tokens — one documented conservative estimator ----

use crate::api::SessionTreeEntry;

use super::tree::entry_text;

pub struct ContextUsageEstimate {
    pub tokens: u64,
    pub usage_tokens: u64,
    pub trailing_tokens: u64,
    pub last_usage_index: Option<usize>,
}

pub fn estimate_tokens(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }
    ((text.chars().count() as f64) / 4.0).ceil() as u64
}

pub fn estimate_context_tokens(entries: &[SessionTreeEntry]) -> ContextUsageEstimate {
    let tokens = entries
        .iter()
        .map(|entry| estimate_tokens(&entry_text(entry)))
        .sum();
    ContextUsageEstimate {
        tokens,
        usage_tokens: 0,
        trailing_tokens: tokens,
        last_usage_index: None,
    }
}
