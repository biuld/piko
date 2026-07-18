//! Island-centric Workbench layout: fixed tree, visibility, sizes.

mod island_tree;
mod state;

#[cfg(test)]
mod tests;

pub use island_tree::{IslandId, prune_island_tree, workbench_island_tree};
pub use state::{
    CENTER_MIN_WIDTH, IslandLayoutState, LayoutBreakpoint, RIGHT_COLUMN_DEFAULT_WIDTH,
    RIGHT_COLUMN_MIN_WIDTH, SESSION_DEFAULT_WIDTH, SESSION_MIN_WIDTH,
};
