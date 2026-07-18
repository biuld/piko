//! First-class Workbench islands: Sessions, Timeline, Composer, Agents, Tree.

pub mod agents;
pub mod composer;
pub mod sessions;
pub mod timeline;
pub mod tree;

#[cfg(test)]
mod tests;

pub use agents::derive_agent_tree;
pub use composer::{ActivityItem, derive_activity, derive_composer};
pub use timeline::derive_timeline;
pub use tree::{default_tree_expansion, derive_conversation_tree, prune_tree_expansion};

pub use agents::AgentsIsland;
pub use composer::ComposerIsland;
pub use sessions::SessionsIsland;
pub use timeline::TimelineIsland;
pub use tree::TreeIsland;
