// ---- Domain: compaction — budget-window trigger, tree projection, file ops ----

pub mod summarizer;

mod file_ops;
mod tokens;
mod tree;
mod trigger;

pub use file_ops::{
    FileOperationLists, FileOperations, compute_file_lists, format_file_operations,
};
pub use tokens::{ContextUsageEstimate, estimate_context_tokens, estimate_tokens};
pub use tree::{
    CutPointResult, active_branch_entries, context_entries_after_compaction, entry_role,
    entry_text, find_cut_point, find_valid_cut_points,
};
pub use trigger::{CompactHoldReason, CompactTrigger, compact_trigger, should_compact};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompactionState {
    pub pending: bool,
    /// Compaction window counter for this session. Advances on every landed
    /// compaction; recorded on the checkpoint entry's `details`.
    pub window_number: u64,
    /// Estimated tokens retained by the most recent compaction (rearm
    /// baseline). None until the first compaction lands.
    pub rearm_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompactionSettings {
    pub enabled: bool,
    pub reserve_tokens: u64,
    pub keep_recent_tokens: u64,
    /// Minimum estimated-token growth since the last compaction before the
    /// next auto-compact may trigger (hysteresis guard).
    pub min_growth_tokens: u64,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            reserve_tokens: 16_384,
            keep_recent_tokens: 20_000,
            min_growth_tokens: 16_384,
        }
    }
}
