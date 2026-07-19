//! Compatibility re-exports for the island layout system.
//!
//! Prefer `crate::shell::workbench::layout` for new code.

pub use crate::shell::workbench::layout::{
    CENTER_MIN_WIDTH, IslandLayoutState as LayoutState, RIGHT_COLUMN_DEFAULT_WIDTH,
    RIGHT_COLUMN_MIN_WIDTH, SESSION_DEFAULT_WIDTH, SESSION_MIN_WIDTH, WINDOW_MIN_HEIGHT,
    WINDOW_MIN_WIDTH,
};
