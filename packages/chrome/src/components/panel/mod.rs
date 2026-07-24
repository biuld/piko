//! Island panel shell: header, body states, scroll viewport, focus ring paint.

#[allow(clippy::module_inception)]
mod panel;
mod state;
#[cfg(test)]
#[path = "state_tests.rs"]
mod state_tests;
mod viewport;

pub use panel::{IslandHeader, IslandPanel};
#[allow(unused_imports)] // public override API for islands
pub use state::{IslandBody, IslandMedia, IslandPlaceholder};
pub use viewport::IslandContentViewport;
