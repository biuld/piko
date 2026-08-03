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

/// Windowless fallback for the hysteresis guard (F-05 slice 2): used when
/// the resolved model window is unavailable or `min_growth_tokens` is
/// neither configured nor derivable.
pub const DEFAULT_MIN_GROWTH_TOKENS: u64 = 16_384;

/// Default ratio of the resolved context window used as the hysteresis
/// guard when `min_growth_tokens` is unset. 12.5% ≈ the documented
/// `16_384` default at a 128k window.
pub const DEFAULT_MIN_GROWTH_FRACTION: f64 = 0.125;

/// Derive the per-model hysteresis guard (F-05 slice 2): `max(1,
/// round(window × fraction))` when a window is resolvable, else the
/// constant fallback. An explicitly configured `min_growth_tokens` is
/// resolved by the caller and never reaches this function.
pub fn min_growth_default(context_window: u64, fraction: Option<f64>) -> u64 {
    match fraction {
        Some(fraction) if context_window > 0 => {
            ((context_window as f64) * fraction).round().max(1.0) as u64
        }
        _ => DEFAULT_MIN_GROWTH_TOKENS,
    }
}

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
            min_growth_tokens: DEFAULT_MIN_GROWTH_TOKENS,
        }
    }
}

#[cfg(test)]
mod defaults_tests {
    use super::{DEFAULT_MIN_GROWTH_TOKENS, min_growth_default};

    #[test]
    fn derives_window_fraction_rounded_to_at_least_one() {
        assert_eq!(min_growth_default(128_000, Some(0.125)), 16_000);
        // Sub-token fractions floor at one token so the guard is never 0.
        assert_eq!(min_growth_default(10, Some(0.01)), 1);
    }

    #[test]
    fn falls_back_to_constant_without_a_window() {
        assert_eq!(
            min_growth_default(0, Some(0.125)),
            DEFAULT_MIN_GROWTH_TOKENS
        );
        assert_eq!(min_growth_default(128_000, None), DEFAULT_MIN_GROWTH_TOKENS);
    }
}
