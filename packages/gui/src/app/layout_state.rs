//! Compatibility re-exports for the island layout system.
//!
//! Prefer `crate::chrome::workbench::layout` for new code.

pub use crate::chrome::workbench::layout::{
    CENTER_MIN_WIDTH, IslandLayoutState as LayoutState, LayoutBreakpoint,
    RIGHT_COLUMN_DEFAULT_WIDTH, RIGHT_COLUMN_MIN_WIDTH, SESSION_DEFAULT_WIDTH, SESSION_MIN_WIDTH,
};
