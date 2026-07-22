//! Agents island: instance tree projection and render.

mod render;
mod view;
mod vm;

pub use view::{
    AgentsConfirm, AgentsIsland, AgentsSelectNext, AgentsSelectPrev, AgentsToggleExpand,
};
#[cfg(test)]
pub(crate) use vm::agent_node_visible;
pub use vm::derive_agent_tree;
