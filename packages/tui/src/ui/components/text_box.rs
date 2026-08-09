use ratatui::{
    layout::Position,
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBox {
    text: String,
    cursor: usize, // Byte offset in UTF-8 string
    mask_char: Option<char>,
    placeholder: String,
}

impl Default for TextBox {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBox {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            mask_char: None,
            placeholder: String::new(),
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

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Display width of the field content before the caret (mask-aware).
    pub fn width_before_cursor(&self) -> usize {
        if let Some(mask) = self.mask_char {
            self.text[..self.cursor.min(self.text.len())]
                .chars()
                .count()
                * unicode_width::UnicodeWidthChar::width(mask).unwrap_or(1)
        } else {
            UnicodeWidthStr::width(&self.text[..self.cursor.min(self.text.len())])
        }
    }

    /// Absolute terminal position of the caret when this field starts at
    /// `origin`. The input component owns caret geometry.
    pub fn caret_position(&self, origin: Position) -> Position {
        Position::new(origin.x + self.width_before_cursor() as u16, origin.y)
    }

    pub fn insert_char(&mut self, ch: char) {
        if ch == '\n' {
            return;
        }
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        let clean = s.replace('\n', "");
        self.text.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
    }

    /// Move the caret to the nearest character boundary at a display column.
    pub fn move_to_column(&mut self, column: u16) {
        let target = usize::from(column);
        let mut width = 0usize;
        self.cursor = self.text.len();
        for (byte, ch) in self.text.char_indices() {
            let char_width = if self.mask_char.is_some() {
                1
            } else {
                unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0)
            };
            if width.saturating_add(char_width) > target {
                self.cursor = byte;
                break;
            }
            width = width.saturating_add(char_width);
        }
    }

    pub fn backspace(&mut self) -> bool {
        let Some(prev) = self.prev_char_boundary(self.cursor) else {
            return false;
        };
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
        true
    }

    fn prev_char_boundary(&self, cursor: usize) -> Option<usize> {
        self.text[..cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
    }

    pub fn render_line(&self, theme: &crate::theme::Theme, focused: bool) -> Line<'static> {
        use crate::ui::components::placeholder_style;
        use ratatui::style::Style;
        if self.text.is_empty() {
            let mut spans = vec![Span::styled(
                self.placeholder.clone(),
                placeholder_style(theme),
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

#[cfg(test)]
mod tests {
    use ratatui::layout::Position;

    use super::TextBox;

    #[test]
    fn masked_caret_uses_terminal_columns_not_utf8_bytes() {
        let mut input = TextBox::new().with_mask('•');
        input.insert_str("abc");

        assert_eq!(input.width_before_cursor(), 3);
        assert_eq!(
            input.caret_position(Position::new(10, 4)),
            Position::new(13, 4)
        );
    }
}
