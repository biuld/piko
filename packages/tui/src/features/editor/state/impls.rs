use super::*;

impl Editor {
    pub fn text(&self) -> String {
        self.text.clone()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn configure(&mut self, config: &EditorConfig) {
        self.history_limit = config.history_limit.max(1);
        self.trim_history();
    }

    pub fn visible_height(&self, config: &EditorConfig, width: u16) -> u16 {
        let content_lines = if config.auto_resize {
            self.visual_lines(width)
                .len()
                .max(1)
                .min(config.max_lines.max(1) as usize) as u16
        } else {
            1
        };
        content_lines + 2
    }

    pub fn cursor_line_col(&self, width: u16, visible_rows: u16) -> (u16, u16) {
        let lines = self.visual_lines(width);
        let index = self.cursor_visual_line_index(&lines);
        let Some(line) = lines.get(index) else {
            return (0, 0);
        };
        let window_start = Self::window_start_for_cursor(index, visible_rows, lines.len());
        let col = UnicodeWidthStr::width(&self.text[line.start..self.cursor.min(line.end)]);
        (index.saturating_sub(window_start) as u16, col as u16)
    }

    pub fn insert_char(&mut self, ch: char) {
        self.exit_history_browse();
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn backspace(&mut self) {
        self.exit_history_browse();
        if self.delete_adjacent_reference_backward() {
            return;
        }
        let Some(prev) = self.prev_char_boundary(self.cursor) else {
            return;
        };
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    pub fn delete(&mut self) {
        self.exit_history_browse();
        if self.delete_adjacent_reference_forward() {
            return;
        }
        let Some(next) = self.next_char_boundary(self.cursor) else {
            return;
        };
        self.text.replace_range(self.cursor..next, "");
    }

    pub fn move_left(&mut self) {
        self.exit_history_browse();
        let line_start = self.current_line_start();
        if self.cursor > line_start
            && let Some(prev) = self.prev_char_boundary(self.cursor)
        {
            self.cursor = prev;
        }
    }

    pub fn move_right(&mut self) {
        self.exit_history_browse();
        let line_end = self.current_line_end();
        if self.cursor < line_end
            && let Some(next) = self.next_char_boundary(self.cursor)
        {
            self.cursor = next;
        }
    }

    pub fn move_line_start(&mut self) {
        self.exit_history_browse();
        self.cursor = self.current_line_start();
    }

    pub fn move_line_end(&mut self) {
        self.exit_history_browse();
        self.cursor = self.current_line_end();
    }

    pub fn take_trimmed(&mut self) -> Option<String> {
        let text = self.expanded_text().trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.push_history(text.clone());
        self.clear();
        Some(text)
    }

    pub fn restore_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
        self.references.clear();
        self.history_index = None;
        self.draft_before_history = None;
    }

    pub fn insert_paste(&mut self, text: &str, config: &EditorConfig) {
        self.exit_history_browse();
        let line_count = text.lines().count();
        if line_count > config.large_paste_lines || text.chars().count() > config.large_paste_chars
        {
            let placeholder = self.next_paste_placeholder(text, line_count);
            self.references.push(ReferenceBlock {
                placeholder: placeholder.clone(),
                content: text.to_string(),
            });
            self.insert_str(&placeholder);
        } else {
            self.insert_str(text);
        }
    }

    pub fn insert_reference_block(&mut self, placeholder: String, content: String) {
        self.exit_history_browse();
        self.references.push(ReferenceBlock {
            placeholder: placeholder.clone(),
            content,
        });
        self.insert_str(&placeholder);
    }

    pub fn replace_range(&mut self, start: usize, end: usize, replacement: &str) {
        self.exit_history_browse();
        let start = clamp_to_char_boundary(&self.text, start.min(self.text.len()));
        let end = clamp_to_char_boundary(&self.text, end.min(self.text.len())).max(start);
        self.text.replace_range(start..end, replacement);
        self.cursor = start + replacement.len();
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.history_index.is_none() {
            self.draft_before_history = Some(self.text.clone());
        }
        let next_index = self
            .history_index
            .map(|index| {
                if index == 0 {
                    self.history.len() - 1
                } else {
                    index - 1
                }
            })
            .unwrap_or_else(|| self.history.len() - 1);
        self.set_from_history(next_index);
    }

    pub fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            let draft = self.draft_before_history.take().unwrap_or_default();
            self.text = draft;
            self.cursor = self.text.len();
            self.history_index = None;
        } else {
            self.set_from_history(index + 1);
        }
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, block: Block<'static>) {
        let visible_rows = area.height.saturating_sub(2).max(1);
        let visual_lines = self.visual_lines(area.width);
        let cursor_index = self.cursor_visual_line_index(&visual_lines);
        let window_start =
            Self::window_start_for_cursor(cursor_index, visible_rows, visual_lines.len());
        let lines = visual_lines
            .into_iter()
            .skip(window_start)
            .take(visible_rows as usize)
            .map(|line| Line::from(Span::raw(self.text[line.start..line.end].to_string())))
            .collect::<Vec<_>>();
        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }

    pub(super) fn cursor_visual_line_index(&self, lines: &[std::ops::Range<usize>]) -> usize {
        lines
            .partition_point(|line| line.start <= self.cursor)
            .saturating_sub(1)
    }

    pub(super) fn window_start_for_cursor(
        cursor_index: usize,
        visible_rows: u16,
        total_lines: usize,
    ) -> usize {
        let visible_rows = visible_rows.max(1) as usize;
        let max_start = total_lines.saturating_sub(visible_rows);
        cursor_index
            .saturating_add(1)
            .saturating_sub(visible_rows)
            .min(max_start)
    }

    pub(super) fn visual_lines(&self, width: u16) -> Vec<std::ops::Range<usize>> {
        let max_width = width.max(1) as usize;
        if self.text.is_empty() {
            return std::iter::once(0..0).collect();
        }

        let mut lines = Vec::new();
        let mut line_start = 0;
        let mut line_width = 0usize;

        for (index, ch) in self.text.char_indices() {
            if ch == '\n' {
                lines.push(line_start..index);
                line_start = index + ch.len_utf8();
                line_width = 0;
                continue;
            }

            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
            if line_width > 0 && line_width + ch_width > max_width {
                lines.push(line_start..index);
                line_start = index;
                line_width = 0;
            }
            line_width += ch_width;
        }

        lines.push(line_start..self.text.len());
        lines
    }

    pub(super) fn insert_str(&mut self, text: &str) {
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    pub(super) fn push_history(&mut self, text: String) {
        if self.history.last() != Some(&text) {
            self.history.push(text);
        }
        self.trim_history();
    }

    pub(super) fn set_from_history(&mut self, index: usize) {
        if let Some(text) = self.history.get(index) {
            self.text = text.clone();
            self.cursor = self.text.len();
            self.history_index = Some(index);
        }
    }

    pub(super) fn trim_history(&mut self) {
        while self.history.len() > self.history_limit {
            self.history.remove(0);
        }
    }

    pub(super) fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.history_index = None;
        self.draft_before_history = None;
        self.references.clear();
        self.next_reference_id = 1;
    }

    pub(super) fn expanded_text(&self) -> String {
        let mut text = self.text.clone();
        for reference in &self.references {
            text = text.replace(&reference.placeholder, &reference.content);
        }
        text
    }

    pub(super) fn exit_history_browse(&mut self) {
        self.history_index = None;
        self.draft_before_history = None;
    }

    pub(super) fn next_paste_placeholder(&mut self, text: &str, line_count: usize) -> String {
        let id = self.next_reference_id;
        self.next_reference_id += 1;
        if line_count > 1 {
            format!("[paste #{id} +{line_count} lines]")
        } else {
            format!("[paste #{id} {} chars]", text.chars().count())
        }
    }

    pub(super) fn delete_adjacent_reference_backward(&mut self) -> bool {
        let Some((start, end)) = self
            .references
            .iter()
            .filter_map(|reference| {
                find_placeholder_before_cursor(&self.text, &reference.placeholder, self.cursor)
            })
            .find(|(_, end)| *end == self.cursor)
        else {
            return false;
        };
        self.replace_range(start, end, "");
        true
    }

    pub(super) fn delete_adjacent_reference_forward(&mut self) -> bool {
        let Some((start, end)) = self
            .references
            .iter()
            .filter_map(|reference| {
                find_placeholder_at_cursor(&self.text, &reference.placeholder, self.cursor)
            })
            .find(|(start, _)| *start == self.cursor)
        else {
            return false;
        };
        self.replace_range(start, end, "");
        true
    }

    pub(super) fn prev_char_boundary(&self, cursor: usize) -> Option<usize> {
        self.text[..cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
    }

    pub(super) fn next_char_boundary(&self, cursor: usize) -> Option<usize> {
        self.text[cursor..]
            .chars()
            .next()
            .map(|ch| cursor + ch.len_utf8())
    }

    pub(super) fn current_line_start(&self) -> usize {
        self.text[..self.cursor]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0)
    }

    pub(super) fn current_line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map(|offset| self.cursor + offset)
            .unwrap_or(self.text.len())
    }
}
