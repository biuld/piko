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

    pub fn insert_char(&mut self, ch: char) {
        self.exit_history_browse();
        self.cursor = self.snap_cursor_out_of_reference(self.cursor, true);
        for reference in &mut self.references {
            if reference.start >= self.cursor {
                reference.start += ch.len_utf8();
            }
        }
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn backspace(&mut self) {
        self.exit_history_browse();
        if self.delete_reference_at_cursor(true) {
            return;
        }
        let Some(prev) = self.prev_char_boundary(self.cursor) else {
            return;
        };
        self.replace_range(prev, self.cursor, "");
    }

    pub fn delete(&mut self) {
        self.exit_history_browse();
        if self.delete_reference_at_cursor(false) {
            return;
        }
        let Some(next) = self.next_char_boundary(self.cursor) else {
            return;
        };
        self.replace_range(self.cursor, next, "");
    }

    pub fn move_left(&mut self) {
        self.exit_history_browse();
        if let Some((start, _)) = self.reference_range_touching_cursor(self.cursor, true) {
            self.cursor = start;
            return;
        }
        let line_start = self.current_line_start();
        if self.cursor > line_start
            && let Some(prev) = self.prev_char_boundary(self.cursor)
        {
            self.cursor = prev;
        }
    }

    pub fn move_right(&mut self) {
        self.exit_history_browse();
        if let Some((_, end)) = self.reference_range_touching_cursor(self.cursor, false) {
            self.cursor = end;
            return;
        }
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

    /// Move the cursor to a visible composer row/column (pointer clicks).
    pub fn move_to_position(&mut self, width: u16, height: u16, col: u16, row: u16) {
        let viewport = self.viewport;
        self.exit_history_browse();
        self.viewport = viewport;
        let visible_rows = height.saturating_sub(2).max(1);
        let layout = self.layout_for_viewport(width, visible_rows);
        let window_start = self.window_start(&layout, visible_rows);
        let content_row = row.saturating_sub(1).min(visible_rows.saturating_sub(1));
        let index = window_start
            .saturating_add(content_row as usize)
            .min(layout.lines.len().saturating_sub(1));
        let Some(line) = layout.lines.get(index) else {
            return;
        };
        let mut target = line.end;
        let mut used = 0u16;
        let policy = crate::terminal::text::TerminalTextPolicy;
        for (offset, grapheme) in policy.grapheme_indices(&self.text[line.start..line.end]) {
            let w = display_width(grapheme) as u16;
            if used + w > col {
                target = line.start + offset;
                break;
            }
            used += w;
        }
        self.cursor = self.snap_cursor_out_of_reference(target, col >= used / 2);
    }

    pub fn move_word_left(&mut self) {
        self.exit_history_browse();
        if let Some((start, _)) = self.reference_range_touching_cursor(self.cursor, true) {
            self.cursor = start;
            return;
        }
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end_matches(|ch: char| !ch.is_alphanumeric() && ch != '_');
        let word = trimmed.trim_end_matches(|ch: char| ch.is_alphanumeric() || ch == '_');
        let target = word.len().max(self.current_line_start());
        self.cursor = self.snap_cursor_out_of_reference(target, false);
    }

    pub fn move_word_right(&mut self) {
        self.exit_history_browse();
        if let Some((_, end)) = self.reference_range_touching_cursor(self.cursor, false) {
            self.cursor = end;
            return;
        }
        let line_end = self.current_line_end();
        let after = &self.text[self.cursor..line_end];
        let skipped = after.trim_start_matches(|ch: char| !ch.is_alphanumeric() && ch != '_');
        let word_end = skipped.trim_start_matches(|ch: char| ch.is_alphanumeric() || ch == '_');
        self.cursor = self.snap_cursor_out_of_reference(line_end - word_end.len(), true);
    }

    pub fn delete_word_backward(&mut self) {
        let end = self.cursor;
        self.move_word_left();
        self.replace_range(self.cursor, end, "");
    }

    pub fn delete_word_forward(&mut self) {
        let start = self.cursor;
        self.move_word_right();
        self.replace_range(start, self.cursor, "");
    }

    pub fn delete_to_line_start(&mut self) {
        let start = self.current_line_start();
        self.replace_range(start, self.cursor, "");
    }

    pub fn delete_to_line_end(&mut self) {
        let end = self.current_line_end();
        self.replace_range(self.cursor, end, "");
    }

    #[cfg(test)]
    pub fn take_trimmed(&mut self) -> Option<String> {
        let submission = self.take_submission()?;
        match submission.content {
            piko_protocol::MessageContent::String(text) => Some(text),
            piko_protocol::MessageContent::Blocks(_) => Some(submission.display_text),
        }
    }

    pub fn restore_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
        self.viewport.reset();
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
            let start = self.cursor;
            self.insert_str(&placeholder);
            self.references.push(ReferenceBlock {
                start,
                placeholder,
                payload: ReferencePayload::Text(text.to_string()),
            });
        } else {
            self.insert_str(text);
        }
    }

    pub fn insert_reference_block(&mut self, placeholder: String, content: String) {
        self.exit_history_browse();
        let start = self.cursor;
        self.insert_str(&placeholder);
        self.references.push(ReferenceBlock {
            start,
            placeholder,
            payload: ReferencePayload::Text(content),
        });
    }

    pub fn replace_range(&mut self, start: usize, end: usize, replacement: &str) {
        self.exit_history_browse();
        let mut start = clamp_to_char_boundary(&self.text, start.min(self.text.len()));
        let mut end = clamp_to_char_boundary(&self.text, end.min(self.text.len())).max(start);
        for (reference_start, reference_end) in self.reference_ranges() {
            if start < reference_end && end > reference_start {
                start = start.min(reference_start);
                end = end.max(reference_end);
            }
        }
        let removed_len = end - start;
        let inserted_len = replacement.len();
        self.references.retain_mut(|reference| {
            let reference_end = reference.start + reference.placeholder.len();
            if start < reference_end && end > reference.start {
                return false;
            }
            if reference.start >= end {
                reference.start = reference.start - removed_len + inserted_len;
            }
            true
        });
        self.text.replace_range(start..end, replacement);
        self.cursor = start + replacement.len();
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.history_index.is_none() {
            self.draft_before_history = Some(self.snapshot_draft());
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

    /// Whether the editor is currently browsing submitted history (a history
    /// browse was entered with `history_prev` and not yet left).
    pub fn is_browsing_history(&self) -> bool {
        self.history_index.is_some()
    }

    pub fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            let draft = self.draft_before_history.take().unwrap_or(EditorDraft {
                text: String::new(),
                cursor: 0,
                references: Vec::new(),
                next_reference_id: 1,
            });
            self.text = draft.text;
            self.cursor = draft.cursor;
            self.references = draft.references;
            self.next_reference_id = draft.next_reference_id;
            self.history_index = None;
            self.viewport.reset();
        } else {
            self.set_from_history(index + 1);
        }
    }

    pub(super) fn insert_str(&mut self, text: &str) {
        for reference in &mut self.references {
            if reference.start >= self.cursor {
                reference.start += text.len();
            }
        }
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    pub(super) fn push_history(&mut self, draft: EditorDraft) {
        if self.history.last() != Some(&draft) {
            self.history.push(draft);
        }
        self.trim_history();
    }

    pub(super) fn set_from_history(&mut self, index: usize) {
        if let Some(draft) = self.history.get(index).cloned() {
            self.text = draft.text;
            self.cursor = draft.cursor;
            self.references = draft.references;
            self.next_reference_id = draft.next_reference_id;
            self.history_index = Some(index);
            self.viewport.reset();
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
        self.viewport.reset();
        self.history_index = None;
        self.draft_before_history = None;
        self.references.clear();
        self.next_reference_id = 1;
    }

    pub(super) fn exit_history_browse(&mut self) {
        self.history_index = None;
        self.draft_before_history = None;
        self.viewport.resume_cursor_follow();
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

    fn delete_reference_at_cursor(&mut self, backward: bool) -> bool {
        let Some((start, end)) = self.reference_range_touching_cursor(self.cursor, backward) else {
            return false;
        };
        self.replace_range(start, end, "");
        true
    }

    pub(super) fn reference_ranges(&self) -> Vec<(usize, usize)> {
        let mut ranges = self
            .references
            .iter()
            .map(|reference| {
                (
                    reference.start,
                    reference.start + reference.placeholder.len(),
                )
            })
            .collect::<Vec<_>>();
        ranges.sort_unstable();
        ranges
    }

    fn reference_range_touching_cursor(
        &self,
        cursor: usize,
        backward: bool,
    ) -> Option<(usize, usize)> {
        self.reference_ranges().into_iter().find(|(start, end)| {
            if backward {
                *start < cursor && cursor <= *end
            } else {
                *start <= cursor && cursor < *end
            }
        })
    }

    fn snap_cursor_out_of_reference(&self, cursor: usize, prefer_end: bool) -> usize {
        self.reference_ranges()
            .into_iter()
            .find(|(start, end)| *start < cursor && cursor < *end)
            .map_or(cursor, |(start, end)| if prefer_end { end } else { start })
    }

    pub(super) fn prev_char_boundary(&self, cursor: usize) -> Option<usize> {
        crate::terminal::text::TerminalTextPolicy.previous_grapheme_boundary(&self.text, cursor)
    }

    pub(super) fn next_char_boundary(&self, cursor: usize) -> Option<usize> {
        crate::terminal::text::TerminalTextPolicy.next_grapheme_boundary(&self.text, cursor)
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
