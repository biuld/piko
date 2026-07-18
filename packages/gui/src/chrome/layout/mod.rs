//! Island-centric Workbench layout: fixed tree, visibility, sizes.

mod state;
mod tree;

#[cfg(test)]
mod tests;

pub use state::{
    AGENTS_TREE_DEFAULT_WIDTH, AGENTS_TREE_MIN_WIDTH, CENTER_MIN_WIDTH, IslandLayoutState,
    LayoutBreakpoint, SESSION_DEFAULT_WIDTH, SESSION_MIN_WIDTH,
};
pub use tree::{IslandId, prune_island_tree, workbench_island_tree};
