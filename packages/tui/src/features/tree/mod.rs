pub mod document;
pub mod format;
pub mod panel;
pub mod render;
pub mod visible;

use std::collections::HashSet;

pub use self::document::TreeDocument;
pub use self::format::{
    session_entry_label, session_entry_preview_text, session_entry_timeline_text,
};
pub use self::visible::{ConnectorKind, TreeFilterMode, VisibleTree};
use crate::ui::components::interactive_workflow::{ChoiceOption, InteractiveWorkflow, Question};

pub fn create_summary_prompt(target_entry_id: String) -> InteractiveWorkflow {
    let question = Question::new(
        "Branch switch",
        "leave current-path entries behind?",
        vec![
            ChoiceOption {
                label: "No summary".into(),
                has_input: false,
                input_prompt: String::new(),
            },
            ChoiceOption {
                label: "Summarize".into(),
                has_input: false,
                input_prompt: String::new(),
            },
            ChoiceOption {
                label: "Custom prompt".into(),
                has_input: true,
                input_prompt: "Custom: ".into(),
            },
        ],
    );
    let mut workflow = InteractiveWorkflow::new(vec![question], false);
    workflow.target_entry_id = Some(target_entry_id);
    workflow
}

use crate::ui::components::text_box::TextBox;

pub struct LabelEditorState {
    pub target_id: String,
    pub input: TextBox,
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
