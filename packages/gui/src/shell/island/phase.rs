//! Shared session-phase → island content state (Empty / Loading / Ready).

use crate::projections::SessionPhaseView;

/// How an island should present itself given the session shell phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IslandSessionPhase {
    /// No session selected — show Empty.
    Idle,
    /// Session open/create/hydrate in flight — show Loading.
    Loading,
    /// Session live — show Ready content (may still be item-empty).
    Ready,
}

impl IslandSessionPhase {
    pub fn from_session_phase(phase: &SessionPhaseView) -> Self {
        match phase {
            SessionPhaseView::IdleNoSession => Self::Idle,
            SessionPhaseView::Opening { .. } | SessionPhaseView::Hydrating { .. } => Self::Loading,
            SessionPhaseView::Live => Self::Ready,
            // Keep structure visible; error copy lives in the center island.
            SessionPhaseView::Error { .. } => Self::Idle,
        }
    }
}
