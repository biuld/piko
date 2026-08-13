//! Token-estimation boundary used by host application bookkeeping.

use piko_protocol::session::SessionTreeEntry;

/// Estimates host-projected entries without coupling host domain policy to
/// the orchestrator runtime that owns the F-04 implementation.
pub trait TranscriptEstimator: Send + Sync {
    fn estimate_entry_tokens(&self, entry: &SessionTreeEntry) -> u64;

    fn estimate_entries_tokens(&self, entries: &[SessionTreeEntry]) -> u64 {
        entries
            .iter()
            .map(|entry| self.estimate_entry_tokens(entry))
            .sum()
    }
}
