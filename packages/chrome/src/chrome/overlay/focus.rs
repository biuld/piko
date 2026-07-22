//! Overlay focus lifecycle helpers (chrome contract; app owns FocusHandles).
//!
//! ## Contract
//!
//! 1. **Open:** app may save island / outer focus, then focus a handle inside
//!    the panel (search field, primary button, …).
//! 2. **While open:** panel root should `track_focus` / receive keys; backdrop
//!    uses `occlude` so pointer hits do not fall through.
//! 3. **Close:** if a session was marked open, restore prior focus (island
//!    Activate or previous handle).
//!
//! Full Tab-cycle trapping depends on GPUI focus containment; chrome documents
//! the session lifecycle so apps do not invent divergent open/close paths.

/// Bookkeeping for one modal / transient overlay focus episode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayFocusSession {
    open: bool,
}

impl OverlayFocusSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(self) -> bool {
        self.open
    }

    /// Mark session open. Returns `true` if this was a fresh open.
    pub fn begin(&mut self) -> bool {
        let was = self.open;
        self.open = true;
        !was
    }

    /// Mark session closed. Returns `true` if restore should run.
    pub fn end(&mut self) -> bool {
        let was = self.open;
        self.open = false;
        was
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_end_restore_flag() {
        let mut s = OverlayFocusSession::new();
        assert!(s.begin());
        assert!(!s.begin()); // already open
        assert!(s.is_open());
        assert!(s.end());
        assert!(!s.end());
        assert!(!s.is_open());
    }
}
