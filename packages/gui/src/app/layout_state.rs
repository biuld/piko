//! Compatibility re-exports for the island layout system.
//!
//! Prefer `crate::chrome::layout` for new code.

pub use crate::chrome::layout::{
    AGENTS_TREE_DEFAULT_WIDTH, AGENTS_TREE_MIN_WIDTH, CENTER_MIN_WIDTH,
    IslandLayoutState as LayoutState, LayoutBreakpoint, SESSION_DEFAULT_WIDTH, SESSION_MIN_WIDTH,
};
