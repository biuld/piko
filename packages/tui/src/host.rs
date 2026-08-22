//! Hostd stdio client shared with other frontends.
//!
//! The spawn/read implementation lives in `piko-comms` so every first-party
//! frontend uses one wire client (ADR-022). The TUI selects its own bridge
//! contract here.

pub use piko_comms::HostLine;

/// Hostd client bound to the TUI process bridge.
pub type HostdClient = piko_comms::HostdClient<piko_comms::contracts::TuiHostBridge>;
