pub mod document;
pub mod format;
pub mod panel;
pub mod render;
pub mod summary_prompt;
pub mod visible;

use std::collections::HashSet;

pub use self::document::TreeDocument;
pub use self::format::{
    session_entry_label, session_entry_preview_text, session_entry_timeline_text,
};
pub use self::summary_prompt::{SummaryChoice, SummaryPromptState};
pub use self::visible::{ConnectorKind, TreeFilterMode, VisibleTree};

pub struct LabelEditorState {
    pub target_id: String,
    pub input: String,
}

pub struct TreePanel {
    pub document: TreeDocument,
    pub visible: VisibleTree,
    pub selection: Option<String>,
    pub filter_mode: TreeFilterMode,
    pub folded: HashSet<String>,
    pub show_label_timestamps: bool,
    pub label_editor: Option<LabelEditorState>,
    pub selected_idx: usize,
}
