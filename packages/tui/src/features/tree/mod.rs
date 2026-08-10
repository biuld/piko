pub mod document;
pub mod format;
pub mod panel;
pub mod render;
pub mod visible;

use std::collections::HashSet;

pub use self::document::TreeDocument;
pub use self::format::{session_entry_label, session_entry_preview_text};
pub use self::render::TreeCtx;
pub use self::visible::{ConnectorKind, TreeFilterMode, VisibleTree};
use crate::ui::components::choice_workflow::{ChoiceOption, ChoiceWorkflow, Question};

pub fn create_summary_prompt(target_entry_id: String) -> ChoiceWorkflow {
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
    let mut workflow = ChoiceWorkflow::new(vec![question], false);
    workflow.target_entry_id = Some(target_entry_id);
    workflow
}

pub enum SummaryPromptConfirm {
    NeedsInput,
    Navigate {
        entry_id: String,
        summarize: bool,
        custom_instructions: Option<String>,
    },
    None,
}

pub fn confirm_summary_prompt(workflow: &mut ChoiceWorkflow) -> SummaryPromptConfirm {
    if workflow.questions.is_empty() {
        return SummaryPromptConfirm::None;
    }

    let active_question = &mut workflow.questions[workflow.active_question_idx];
    let choice = &active_question.choices[active_question.selected_idx];

    if choice.has_input && !active_question.is_input_active {
        active_question.is_input_active = true;
        return SummaryPromptConfirm::NeedsInput;
    }

    let mut summarize = false;
    let mut custom_instructions = None;

    match active_question.selected_idx {
        0 => {}
        1 => summarize = true,
        2 => {
            summarize = true;
            custom_instructions = Some(active_question.input_value.text().to_string());
        }
        _ => {}
    }

    let Some(entry_id) = workflow.target_entry_id.clone() else {
        return SummaryPromptConfirm::None;
    };

    SummaryPromptConfirm::Navigate {
        entry_id,
        summarize,
        custom_instructions,
    }
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
    pub filter: String,
    pub filter_mode: TreeFilterMode,
    pub folded: HashSet<String>,
    /// Restrict the visible tree to one agent instance's entries. `None` shows
    /// all agents (session-level entries are always visible).
    pub agent_filter: Option<String>,
    pub show_label_timestamps: bool,
    pub label_editor: Option<LabelEditorState>,
    pub selected_idx: usize,
}
