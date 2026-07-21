//! Pure ClientState → view-model projections (no GPUI).
//!
//! Session phase, sidebar rows, and status bar summary. Island-specific
//! projections live under `crate::features`.

mod sidebar_filter;
mod view_model;

#[cfg(test)]
mod tests;

pub use sidebar_filter::apply_sidebar_search;
pub use view_model::{
    ConnectionStatus, SessionPhaseView, SessionRow, SessionRowKind, SidebarGroup, SidebarPrefs,
    SidebarViewModel, StatusBarViewModel, derive_phase_view, derive_sidebar, derive_status_bar,
    normalize_cwd_key,
};
