//! Phase 4 conversation workbench: timeline, composer, agent tree, activity.

pub mod activity_vm;
pub mod agent_tree_vm;
pub mod composer_vm;
pub mod render;
pub mod timeline_vm;
pub mod tree_render;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod stress_tests;

pub use activity_vm::{ActivityItem, ActivityItemKind, ActivityViewModel, derive_activity};
pub use agent_tree_vm::{AgentTreeNode, AgentTreeViewModel, derive_agent_tree};
pub use composer_vm::{ComposerViewModel, derive_composer};
pub use timeline_vm::{
    TimelineRow, TimelineRowKind, TimelineViewModel, ToolCardStatus, derive_timeline,
};
pub use tree_render::render_agent_tree_panel;
