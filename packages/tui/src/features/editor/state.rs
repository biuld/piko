use std::cell::RefCell;

use ratatui::widgets::Block;
use ratatui_textarea::{CursorMove, TextArea};

/// Editor wraps ratatui-textarea's `TextArea` with input history support.
///
/// Uses `RefCell` for interior mutability so the TextArea block can be
/// configured from `&self` rendering contexts.
#[derive(Default)]
pub struct Editor {
    ta: RefCell<TextArea<'static>>,
    history: Vec<String>,
    history_index: Option<usize>,
}

impl Editor {
    // ── text access ───────────────────────────────────────────────────────

    /// Current content as a single string (lines joined by newline).
    pub fn text(&self) -> String {
        self.ta.borrow().lines().join("\n")
    }

    /// Byte offset of the cursor in the full text.
    pub fn cursor(&self) -> usize {
        let ta = self.ta.borrow();
        let c = ta.cursor();
        let mut offset = 0usize;
        for (i, line) in ta.lines().iter().enumerate() {
            if i == c.0 {
                offset += line[..c.1.min(line.len())].len();
                break;
            }
            offset += line.len() + 1;
        }
        offset
    }

    pub fn is_empty(&self) -> bool {
        self.ta.borrow().lines().iter().all(|l| l.is_empty())
    }

    /// (row, col) in u16 for terminal cursor positioning.
    pub fn cursor_line_col(&self) -> (u16, u16) {
        let c = self.ta.borrow().cursor();
        (c.0 as u16, c.1 as u16)
    }

    // ── editing ───────────────────────────────────────────────────────────

    pub fn insert_char(&mut self, ch: char) {
        self.ta.borrow_mut().insert_char(ch);
        self.history_index = None;
    }

    pub fn insert_newline(&mut self) {
        self.ta.borrow_mut().insert_newline();
        self.history_index = None;
    }

    pub fn backspace(&mut self) {
        self.ta.borrow_mut().move_cursor(CursorMove::Back);
        self.ta.borrow_mut().delete_str(1);
        self.history_index = None;
    }

    pub fn delete(&mut self) {
        self.ta.borrow_mut().delete_str(1);
        self.history_index = None;
    }

    pub fn move_left(&mut self) {
        let mut ta = self.ta.borrow_mut();
        if ta.cursor().1 > 0 {
            ta.move_cursor(CursorMove::Back);
        }
    }

    pub fn move_right(&mut self) {
        let ta = self.ta.borrow();
        let c = ta.cursor();
        let line_len = ta.lines().get(c.0).map(|l| l.len()).unwrap_or(0);
        drop(ta);
        if c.1 < line_len {
            self.ta.borrow_mut().move_cursor(CursorMove::Forward);
        }
    }

    pub fn move_line_start(&mut self) {
        self.ta.borrow_mut().move_cursor(CursorMove::Head);
    }

    pub fn move_line_end(&mut self) {
        self.ta.borrow_mut().move_cursor(CursorMove::End);
    }

    // ── submit / history ──────────────────────────────────────────────────

    pub fn take_trimmed(&mut self) -> Option<String> {
        let text = self.ta.borrow().lines().join("\n").trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.push_history(text.clone());
        *self.ta.borrow_mut() = TextArea::default();
        self.history_index = None;
        Some(text)
    }

    pub fn replace_range(&mut self, _start: usize, _end: usize, replacement: &str) {
        self.ta.borrow_mut().insert_str(replacement);
        self.history_index = None;
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next_index = self
            .history_index
            .map(|index| index.saturating_sub(1))
            .unwrap_or_else(|| self.history.len() - 1);
        self.set_from_history(next_index);
    }

    pub fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            *self.ta.borrow_mut() = TextArea::default();
            self.history_index = None;
        } else {
            self.set_from_history(index + 1);
        }
    }

    fn push_history(&mut self, text: String) {
        if self.history.last() != Some(&text) {
            self.history.push(text);
        }
        while self.history.len() > 100 {
            self.history.remove(0);
        }
    }

    fn set_from_history(&mut self, index: usize) {
        if let Some(text) = self.history.get(index) {
            let mut ta = TextArea::default();
            for line in text.lines() {
                ta.insert_str(line);
                ta.insert_newline();
            }
            let c = ta.cursor();
            if c.0 > 0 && !text.ends_with('\n') {
                ta.move_cursor(CursorMove::End);
                ta.delete_str(1);
            }
            *self.ta.borrow_mut() = ta;
            self.history_index = Some(index);
        }
    }

    // ── rendering ─────────────────────────────────────────────────────────

    /// Set the block (border) on the TextArea for rendering.
    pub fn set_block(&self, block: Block<'static>) {
        self.ta.borrow_mut().set_block(block);
    }

    /// Render the TextArea widget into the given frame and area.
    pub fn render(&self, frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect) {
        let ta = self.ta.borrow();
        frame.render_widget(&*ta, area);
    }
}
