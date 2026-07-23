//! Product vertical slices (UI + local VM + feature actions).
//!
//! Shell mounts these; features must not depend on sibling feature internals.

pub mod agents;
pub mod composer;
pub mod palette;
pub mod prompts;
pub mod sessions;
pub mod settings;
pub mod timeline;
pub mod tree;

#[cfg(test)]
mod island_tests;

pub use agents::AgentsIsland;
pub use agents::derive_agent_tree;
pub use agents::{AgentsConfirm, AgentsSelectNext, AgentsSelectPrev, AgentsToggleExpand};
pub use composer::ComposerIsland;
pub use composer::{derive_activity, derive_composer};
pub use palette::{CommandPalette, PaletteConfirm, PaletteSelectNext, PaletteSelectPrev};
pub use prompts::{
    InteractionForm, PromptFront, PromptKind, approval_title, derive_prompt_front,
    interaction_title, render_approval_body,
};
pub use sessions::{
    ClearSessionSearch, SessionsConfirm, SessionsIsland, SessionsSelectNext, SessionsSelectPrev,
    SessionsToggleFocused,
};
pub use settings::{
    ConfirmSection, SETTINGS_FOCUS_ORDER, SelectNextSection, SelectPrevSection, SettingsIslandId,
    SettingsNavIsland, SettingsPanelIsland, SettingsSection,
};
pub use timeline::TimelineIsland;
pub use timeline::derive_timeline;
pub use tree::{TreeConfirm, TreeIsland, TreeSelectNext, TreeSelectPrev, TreeToggleFocused};
pub use tree::{default_tree_expansion, derive_conversation_tree, prune_tree_expansion};
