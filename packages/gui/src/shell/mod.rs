//! Phase-driven view models derived from `ClientState`.
//!
//! These are pure, testable types with no GPUI dependency. GPUI views read
//! these structs to decide what to render. The mapping is deterministic:
//! `ClientState` → view model.

pub mod view_model;

#[cfg(test)]
mod tests;

pub use view_model::{
    ConnectionStatus, SessionPhaseView, SessionRowKind, StatusBarViewModel, derive_phase_view,
    derive_sidebar, derive_status_bar,
};
