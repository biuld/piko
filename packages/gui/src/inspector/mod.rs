//! Right Inspector: Agent Tree + Conversation Map.

mod conversation_map_vm;
mod render;

pub use conversation_map_vm::{
    ConversationMapViewModel, MapEntryKind, MapNode, default_map_expansion,
    derive_conversation_map, prune_expansion,
};
pub use render::{
    InspectorHandlers, InspectorSheetHandlers, render_inspector_panel, render_inspector_sheet_body,
};
