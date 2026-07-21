//! Shared presentational widgets reused across Workbench islands.

mod tree_list;

pub use tree_list::{
    TreeClickHandler, TreeContextMenuBuilder, TreeRowAccessory, TreeRowSpec, render_tree_list,
};
