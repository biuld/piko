//! Pure-ish runtime: archipelago navigation, island contracts, layout trees.
//!
//! Prefer keeping GPUI *paint* out of here when possible; panel/overlay live under
//! [`crate::chrome`].

pub mod archipelago;
pub mod island;
pub mod layout;
