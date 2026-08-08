use crate::api::SessionTreeEntry;
use crate::application::host_app::HostApp;
use crate::application::sessions::helpers::session_reconciled_message;
use crate::domain::compaction::{
    CompactTrigger, CompactionSettings, DEFAULT_MIN_GROWTH_FRACTION, DEFAULT_MIN_GROWTH_TOKENS,
    active_branch_entries, compact_trigger, context_entries_after_compaction,
    estimate_context_tokens, min_growth_default,
};
use crate::util::{ClientEventSender, send_event};
use piko_protocol::command::CompactMode;

/// Fixed checkpoint message for a token-budget compact (F-05): history is
/// dropped without a model summarization call.
pub const NEW_CONTEXT_WINDOW_MESSAGE: &str =
    "A new context window was started without summarizing conversation history.";

/// Resolve the hysteresis guard (F-05 slice 2): an explicitly configured
/// `min_growth_tokens` wins; otherwise derive it from the resolved context
/// window via the fraction (defaulting to `DEFAULT_MIN_GROWTH_FRACTION`).
pub(crate) fn effective_min_growth_tokens(
    configured: Option<u64>,
    fraction: Option<f64>,
    context_window: u64,
) -> u64 {
    configured.unwrap_or_else(|| {
        min_growth_default(
            context_window,
            Some(fraction.unwrap_or(DEFAULT_MIN_GROWTH_FRACTION)),
        )
    })
}

mod compact;
#[cfg(test)]
mod tests;
