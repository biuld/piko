use crate::{config::editor::EditorConfig, features::auto_completion::AutoComplete};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

pub struct Editor {
    text: String,
    cursor: usize,
    viewport: EditorViewport,
    history: Vec<EditorDraft>,
    history_index: Option<usize>,
    draft_before_history: Option<EditorDraft>,
    references: Vec<ReferenceBlock>,
    next_reference_id: usize,
    history_limit: usize,
    pub auto_complete: AutoComplete,
}

#[derive(Clone, Debug, PartialEq)]
struct ReferenceBlock {
    start: usize,
    placeholder: String,
    payload: ReferencePayload,
}

#[derive(Clone, Debug, PartialEq)]
enum ReferencePayload {
    Text(String),
    Image { data: String, mime_type: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorDraft {
    text: String,
    cursor: usize,
    references: Vec<ReferenceBlock>,
    next_reference_id: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorSubmission {
    pub content: piko_protocol::MessageContent,
    pub display_text: String,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            viewport: EditorViewport::default(),
            history: Vec::new(),
            history_index: None,
            draft_before_history: None,
            references: Vec::new(),
            next_reference_id: 1,
            history_limit: 100,
            auto_complete: AutoComplete::new(),
        }
    }
}

mod impls;
mod render;
mod submission;
#[cfg(test)]
mod tests;
mod viewport;

use viewport::EditorViewport;

fn clamp_to_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}
