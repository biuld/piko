//! Timeline island: conversation document projection and scroll viewport.

mod markdown_cache;
mod render;
mod view;
mod vm;

#[cfg(test)]
mod stress_tests;

pub use view::TimelineIsland;
pub use vm::derive_timeline;

#[cfg(test)]
pub(crate) use vm::{
    TimelineRow, TimelineRowKind, ToolCardStatus, VisualSender, group_timeline_rows,
};
