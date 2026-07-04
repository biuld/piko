use ratatui::text::{Line, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBox {
    text: String,
    cursor: usize, // Byte offset in UTF-8 string
    mask_char: Option<char>,
    placeholder: String,
    multiline: bool,
}

impl Default for TextBox {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl TextBox {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            mask_char: None,
            placeholder: String::new(),
            multiline: false,
        }
    }

    pub fn with_mask(mut self, mask: char) -> Self {
        self.mask_char = Some(mask);
        self
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn with_multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: String) {
        self.text = text;
        self.cursor = self.text.len();
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn insert_char(&mut self, ch: char) {
        if !self.multiline && ch == '\n' {
            return;
        }
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        let mut clean_s = s.to_string();
        if !self.multiline {
            clean_s = clean_s.replace('\n', "");
        }
        self.text.insert_str(self.cursor, &clean_s);
        self.cursor += clean_s.len();
    }

    pub fn replace_range(&mut self, start: usize, end: usize, replacement: &str) {
        let start = self.clamp_to_char_boundary(start.min(self.text.len()));
        let end = self
            .clamp_to_char_boundary(end.min(self.text.len()))
            .max(start);

        let mut clean_replacement = replacement.to_string();
        if !self.multiline {
            clean_replacement = clean_replacement.replace('\n', "");
        }

        self.text.replace_range(start..end, &clean_replacement);
        self.cursor = start + clean_replacement.len();
    }

    pub fn backspace(&mut self) -> bool {
        let Some(prev) = self.prev_char_boundary(self.cursor) else {
            return false;
        };
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
        true
    }

    pub fn delete_forward(&mut self) -> bool {
        let Some(next) = self.next_char_boundary(self.cursor) else {
            return false;
        };
        self.text.replace_range(self.cursor..next, "");
        true
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0
            && let Some(prev) = self.prev_char_boundary(self.cursor)
        {
            self.cursor = prev;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.text.len()
            && let Some(next) = self.next_char_boundary(self.cursor)
        {
            self.cursor = next;
        }
    }

    pub fn move_start(&mut self) {
        if self.multiline {
            self.cursor = self.current_line_start();
        } else {
            self.cursor = 0;
        }
    }

    pub fn move_end(&mut self) {
        if self.multiline {
            self.cursor = self.current_line_end();
        } else {
            self.cursor = self.text.len();
        }
    }

    pub fn cursor_position(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = self.clamp_to_char_boundary(cursor.min(self.text.len()));
    }

    fn prev_char_boundary(&self, cursor: usize) -> Option<usize> {
        self.text[..cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
    }

    fn next_char_boundary(&self, cursor: usize) -> Option<usize> {
        self.text[cursor..]
            .chars()
            .next()
            .map(|ch| cursor + ch.len_utf8())
    }

    fn clamp_to_char_boundary(&self, mut index: usize) -> usize {
        while index > 0 && !self.text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    fn current_line_start(&self) -> usize {
        self.text[..self.cursor]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0)
    }

    fn current_line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map(|offset| self.cursor + offset)
            .unwrap_or(self.text.len())
    }

    pub fn render_line(&self, theme: &crate::theme::Theme, focused: bool) -> Line<'static> {
        use ratatui::style::Style;
        if self.text.is_empty() {
            let mut spans = vec![Span::styled(
                self.placeholder.clone(),
                Style::default().fg(theme.muted),
            )];
            if focused {
                spans.push(Span::styled("█", Style::default().fg(theme.accent)));
            }
            Line::from(spans)
        } else {
            let cursor = self.cursor;

            let display_text = if let Some(mask) = self.mask_char {
                let char_count =
                    self.text[..cursor].chars().count() + self.text[cursor..].chars().count();
                mask.to_string().repeat(char_count)
            } else {
                self.text.clone()
            };

            let cursor_byte_in_display = if let Some(mask) = self.mask_char {
                // cursor_char_idx is a *char count*, convert to byte offset for display_text
                let char_count_before = self.text[..cursor].chars().count();
                char_count_before * mask.len_utf8()
            } else {
                cursor
            };

            let (before, at_or_after) = if self.mask_char.is_some() {
                (
                    &display_text[..cursor_byte_in_display],
                    &display_text[cursor_byte_in_display..],
                )
            } else {
                (&self.text[..cursor], &self.text[cursor..])
            };

            let mut spans = vec![Span::styled(
                before.to_string(),
                Style::default().fg(theme.text),
            )];

            if focused {
                let mut after_chars = at_or_after.chars();
                if let Some(ch) = after_chars.next() {
                    spans.push(Span::styled(
                        ch.to_string(),
                        Style::default().fg(theme.text).bg(theme.accent),
                    ));
                    let remaining: String = after_chars.collect();
                    spans.push(Span::styled(remaining, Style::default().fg(theme.text)));
                } else {
                    spans.push(Span::styled("█", Style::default().fg(theme.accent)));
                }
            } else {
                spans.push(Span::styled(
                    at_or_after.to_string(),
                    Style::default().fg(theme.text),
                ));
            }

            Line::from(spans)
        }
    }
}
