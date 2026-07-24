//! Island-centric Workbench layout: fixed tree, visibility, sizes.
//!
//! Generic split/prune: [`piko_chrome::runtime::layout`]. Product ids, default tree, and
//! dock-fit constants live here.

mod island_tree;
mod state;

#[cfg(test)]
mod tests;

pub use island_tree::{ALL_ISLAND_IDS, IslandId, prune_island_tree, workbench_island_tree};
pub use state::{
    CENTER_MIN_WIDTH, IslandLayoutState, RIGHT_COLUMN_DEFAULT_WIDTH, RIGHT_COLUMN_MIN_WIDTH,
    SESSION_DEFAULT_WIDTH, SESSION_MIN_WIDTH, WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH,
};
