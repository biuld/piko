//! Host-owned session bookkeeping (F-32 / D-44).
//!
//! Incurred token/cost facts stay journal-backed. Occupancy is a conservative
//! F-04 estimate of the host-projected session tree. Compaction policy and
//! live preflight stay elsewhere.

mod ledger;
mod occupancy;
mod projection;

pub use occupancy::{
    ContextOccupancy, ContextUsageEstimate, estimate_context_tokens, estimate_entry_tokens,
    estimate_tokens, occupancy,
};
