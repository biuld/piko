//! Timeline island: conversation document projection and scroll viewport.

mod render;
mod view;
mod vm;

#[cfg(test)]
mod stress_tests;

pub use view::TimelineIsland;
pub use vm::derive_timeline;

#[cfg(test)]
pub(crate) use vm::{TimelineRowKind, ToolCardStatus};
