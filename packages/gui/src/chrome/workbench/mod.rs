//! Workbench column assembly: left (Sessions) | center | right (Agents ↕ Tree).

pub mod layout;

mod assemble;
mod center;
mod left;
mod right;

pub use layout::{IslandId, prune_island_tree, workbench_island_tree};
pub use right::render_right_column;
