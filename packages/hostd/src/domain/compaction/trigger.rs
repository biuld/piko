// ---- Domain: compaction trigger — budget-window decision with hysteresis ----

use super::CompactionSettings;
use super::CompactionState;
use super::tokens::{ContextUsageEstimate, estimate_context_tokens};
use crate::api::SessionTreeEntry;

/// Why an auto-compact decision held off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactHoldReason {
    UnderHighWaterline,
    InsufficientGrowth,
}

/// Outcome of the budget-window auto-compact decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactTrigger {
    Trigger,
    Hold(CompactHoldReason),
    Disabled,
}

/// Budget-window trigger with hysteresis (F-05 / D-05).
///
/// A session compacts at most once per window: the first time the branch
/// crosses the high waterline (`window − reserve`), then only again after
/// the estimate has grown by at least `min_growth_tokens` beyond the rearm
/// baseline recorded by the last compaction. The estimate and the rearm
/// baseline share the F-04 estimator so the decision and the dispatched
/// transcript can never diverge.
pub fn compact_trigger(
    estimate: &ContextUsageEstimate,
    context_window: u64,
    settings: &CompactionSettings,
    state: &CompactionState,
) -> CompactTrigger {
    if !settings.enabled {
        return CompactTrigger::Disabled;
    }
    if estimate.tokens.saturating_add(settings.reserve_tokens) <= context_window {
        return CompactTrigger::Hold(CompactHoldReason::UnderHighWaterline);
    }
    match state.rearm_tokens {
        None => CompactTrigger::Trigger,
        Some(rearm) if estimate.tokens.saturating_sub(rearm) >= settings.min_growth_tokens => {
            CompactTrigger::Trigger
        }
        Some(_) => CompactTrigger::Hold(CompactHoldReason::InsufficientGrowth),
    }
}

/// Legacy single-threshold predicate: true when the first-window trigger
/// would fire. Kept for callers that only need the waterline check.
pub fn should_compact(
    entries: &[SessionTreeEntry],
    context_window: u64,
    settings: &CompactionSettings,
) -> bool {
    let estimate = estimate_context_tokens(entries);
    matches!(
        compact_trigger(
            &estimate,
            context_window,
            settings,
            &CompactionState::default()
        ),
        CompactTrigger::Trigger
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn estimate(tokens: u64) -> ContextUsageEstimate {
        ContextUsageEstimate {
            tokens,
            usage_tokens: 0,
            trailing_tokens: tokens,
            last_usage_index: None,
        }
    }

    fn settings() -> CompactionSettings {
        CompactionSettings {
            enabled: true,
            reserve_tokens: 16_384,
            keep_recent_tokens: 20_000,
            min_growth_tokens: 16_384,
        }
    }

    #[test]
    fn trigger_respects_enabled_and_high_waterline() {
        let s = settings();
        let window = 128_000;
        // Below the waterline: hold regardless of window state.
        assert_eq!(
            compact_trigger(&estimate(100_000), window, &s, &CompactionState::default()),
            CompactTrigger::Hold(CompactHoldReason::UnderHighWaterline)
        );
        // Disabled wins over an over-budget branch.
        let disabled = CompactionSettings {
            enabled: false,
            ..settings()
        };
        assert_eq!(
            compact_trigger(
                &estimate(200_000),
                window,
                &disabled,
                &CompactionState::default()
            ),
            CompactTrigger::Disabled
        );
    }

    #[test]
    fn first_window_triggers_once_past_waterline() {
        let s = settings();
        let window = 128_000;
        // Over the waterline with no prior compaction: trigger.
        assert_eq!(
            compact_trigger(&estimate(112_000), window, &s, &CompactionState::default()),
            CompactTrigger::Trigger
        );
    }

    #[test]
    fn rearm_baseline_requires_minimum_growth() {
        let s = settings();
        let window = 128_000;
        // Compaction retained 20_000 tokens; branch grew to 112_000 — growth
        // 92_000 ≥ min_growth: trigger again.
        let grown = CompactionState {
            pending: false,
            window_number: 1,
            rearm_tokens: Some(20_000),
        };
        assert_eq!(
            compact_trigger(&estimate(112_000), window, &s, &grown),
            CompactTrigger::Trigger
        );

        // Same rearm baseline but the branch barely grew: hysteresis holds.
        let stale = CompactionState {
            pending: false,
            window_number: 1,
            rearm_tokens: Some(100_000),
        };
        assert_eq!(
            compact_trigger(&estimate(112_000), window, &s, &stale),
            CompactTrigger::Hold(CompactHoldReason::InsufficientGrowth)
        );
    }

    #[test]
    fn should_compact_matches_first_window_trigger() {
        let s = settings();
        assert!(!should_compact(&[], 128_000, &s));
    }
}
