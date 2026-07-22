//! Island runtime contracts: focus ownership, host messaging, focus table.
//!
//! Panel *paint* lives under [`crate::chrome::panel`]. Product island ids and
//! domain messages live in the consuming app.

mod contract;
mod focus;

pub use contract::{
    FocusMsg, IslandFocusSlot, IslandFocusTable, IslandHost, IslandMessage, IslandView,
    UnknownIsland, activate_focus_handle, route_focus_message, schedule_island_message,
};
pub use focus::{FocusCycleDir, FocusReason, FocusRing, FocusTransition};
