use super::*;

impl Editor {
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
        let col = display_width(&self.text[line.start..self.cursor.min(line.end)]);
        (index.saturating_sub(window_start) as u16, col as u16)
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
        frame.render_widget(Paragraph::new(lines).block(block), area);
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
        let reference_ranges = self.reference_ranges();
        let mut cursor = 0usize;
        let policy = crate::terminal::text::TerminalTextPolicy;
        while cursor < self.text.len() {
            if let Some((_, end)) = reference_ranges.iter().find(|(start, _)| *start == cursor) {
                let atom_width = display_width(&self.text[cursor..*end]).max(1);
                if line_width > 0 && line_width + atom_width > max_width {
                    lines.push(line_start..cursor);
                    line_start = cursor;
                    line_width = 0;
                }
                line_width += atom_width;
                cursor = *end;
                continue;
            }
            let (offset, grapheme) = policy
                .grapheme_indices(&self.text[cursor..])
                .next()
                .expect("valid grapheme");
            let index = cursor + offset;
            if grapheme == "\n" {
                lines.push(line_start..index);
                line_start = index + grapheme.len();
                line_width = 0;
                cursor = line_start;
                continue;
            }
            let ch_width = display_width(grapheme).max(1);
            if line_width > 0 && line_width + ch_width > max_width {
                lines.push(line_start..index);
                line_start = index;
                line_width = 0;
            }
            line_width += ch_width;
            cursor = index + grapheme.len();
        }
        lines.push(line_start..self.text.len());
        lines
    }
}
