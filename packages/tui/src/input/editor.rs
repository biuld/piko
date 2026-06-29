#[derive(Default)]
pub struct Editor {
    text: String,
    cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
}

impl Editor {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.history_index = None;
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = previous_boundary(&self.text, self.cursor);
        self.text.replace_range(previous..self.cursor, "");
        self.cursor = previous;
        self.history_index = None;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = next_boundary(&self.text, self.cursor);
        self.text.replace_range(self.cursor..next, "");
        self.history_index = None;
    }

    pub fn move_left(&mut self) {
        self.cursor = previous_boundary(&self.text, self.cursor);
    }

    pub fn move_right(&mut self) {
        self.cursor = next_boundary(&self.text, self.cursor);
    }

    pub fn move_line_start(&mut self) {
        self.cursor = self.text[..self.cursor]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
    }

    pub fn move_line_end(&mut self) {
        self.cursor = self
            .text
            .get(self.cursor..)
            .and_then(|tail| tail.find('\n').map(|index| self.cursor + index))
            .unwrap_or(self.text.len());
    }

    pub fn take_trimmed(&mut self) -> Option<String> {
        let text = self.text.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.push_history(text.clone());
        self.text.clear();
        self.cursor = 0;
        self.history_index = None;
        Some(text)
    }

    pub fn replace_range(&mut self, start: usize, end: usize, replacement: &str) {
        if start > end || end > self.text.len() {
            return;
        }
        self.text.replace_range(start..end, replacement);
        self.cursor = start + replacement.len();
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
            self.text.clear();
            self.cursor = 0;
            self.history_index = None;
        } else {
            self.set_from_history(index + 1);
        }
    }

    pub fn cursor_line_col(&self) -> (u16, u16) {
        let before = &self.text[..self.cursor];
        let row = before.bytes().filter(|byte| *byte == b'\n').count() as u16;
        let col = before
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count() as u16;
        (row, col)
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
            self.text = text.clone();
            self.cursor = self.text.len();
            self.history_index = Some(index);
        }
    }
}

fn previous_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_boundary(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| cursor + index)
        .unwrap_or(text.len())
}
