//! Pure ClientState → view-model projections (no GPUI).
//!
//! Session phase, sidebar rows, and status bar summary. Island-specific
//! projections live under `crate::islands`.

mod view_model;

#[cfg(test)]
mod tests;

pub use view_model::{
    ConnectionStatus, SessionPhaseView, SessionRow, SessionRowKind, SidebarGroup, SidebarViewModel,
    StatusBarViewModel, derive_phase_view, derive_sidebar, derive_status_bar, normalize_cwd_key,
};
