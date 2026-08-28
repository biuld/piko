use std::time::Instant;

use super::ThoughtPhase;

/// A compact animation family reserved for Timeline thought rows. The
/// Bottom Bar uses a braille family, so the two feedback channels remain
/// visually distinguishable.
pub(crate) const THOUGHT_SPINNER: [char; 4] = ['◐', '◓', '◑', '◒'];

pub(crate) fn elapsed_ms(observed_at: Instant, now: Instant) -> u64 {
    now.saturating_duration_since(observed_at)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub(crate) fn phase_duration_ms(phase: ThoughtPhase, now: Instant) -> Option<u64> {
    match phase {
        ThoughtPhase::Streaming { observed_at } => Some(elapsed_ms(observed_at, now)),
        ThoughtPhase::Completed { duration_ms } => duration_ms,
    }
}

pub(crate) fn format_duration_ms(duration_ms: u64) -> String {
    let seconds = duration_ms / 1_000;
    if seconds < 60 {
        format!("{seconds}.{}s", (duration_ms % 1_000) / 100)
    } else {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    }
}
