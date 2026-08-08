use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{config::editor::EditorConfig, features::auto_completion::AutoComplete};

pub struct Editor {
    text: String,
    cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    draft_before_history: Option<String>,
    references: Vec<ReferenceBlock>,
    next_reference_id: usize,
    history_limit: usize,
    pub auto_complete: AutoComplete,
}

#[derive(Clone)]
struct ReferenceBlock {
    placeholder: String,
    content: String,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
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
#[cfg(test)]
mod tests;

fn clamp_to_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn find_placeholder_before_cursor(
    text: &str,
    placeholder: &str,
    cursor: usize,
) -> Option<(usize, usize)> {
    let before = text.get(..cursor)?;
    before
        .rfind(placeholder)
        .filter(|start| *start + placeholder.len() == cursor)
        .map(|start| (start, cursor))
}

fn find_placeholder_at_cursor(
    text: &str,
    placeholder: &str,
    cursor: usize,
) -> Option<(usize, usize)> {
    text.get(cursor..)?
        .starts_with(placeholder)
        .then_some((cursor, cursor + placeholder.len()))
}
