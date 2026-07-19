//! Workbench Primary Surface: TitleBar, body columns, StatusBar.

pub mod layout;

mod assemble;
mod center;
mod frame;
mod left;
mod right;
mod status_bar;
pub(crate) mod title_bar;

pub use frame::mount_frame;
pub use layout::{IslandId, prune_island_tree, workbench_island_tree};
pub use right::render_right_column;
pub(crate) use status_bar::render_status_bar;
pub(crate) use title_bar::render_title_bar;
