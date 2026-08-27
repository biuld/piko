use super::*;
use crate::theme::Theme;
use ratatui::style::Style;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

pub(super) struct EditorLayout {
    pub(super) lines: Vec<std::ops::Range<usize>>,
    pub(super) show_scrollbar: bool,
}

impl Editor {
    pub fn visible_height(&self, config: &EditorConfig, width: u16) -> u16 {
        let content_lines = if config.auto_resize {
            self.layout_for_viewport(width, config.max_lines.max(1))
                .lines
                .len()
                .max(1)
                .min(config.max_lines.max(1) as usize) as u16
        } else {
            1
        };
        content_lines + 2
    }

    pub fn cursor_line_col(&self, width: u16, visible_rows: u16) -> (u16, u16) {
        let layout = self.layout_for_viewport(width, visible_rows);
        let index = self.cursor_visual_line_index(&layout.lines);
        let Some(line) = layout.lines.get(index) else {
            return (0, 0);
        };
        let window_start = self.window_start(&layout, visible_rows);
        let col = display_width(&self.text[line.start..self.cursor.min(line.end)]);
        (index.saturating_sub(window_start) as u16, col as u16)
    }

    pub(crate) fn cursor_is_visible(&self, width: u16, visible_rows: u16) -> bool {
        let layout = self.layout_for_viewport(width, visible_rows);
        let index = self.cursor_visual_line_index(&layout.lines);
        let window_start = self.window_start(&layout, visible_rows);
        index >= window_start
            && index < window_start.saturating_add(usize::from(visible_rows.max(1)))
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, block: Block<'static>, theme: &Theme) {
        let inner = block.inner(area);
        let visible_rows = inner.height.max(1);
        let layout = self.layout_for_viewport(inner.width, visible_rows);
        let window_start = self.window_start(&layout, visible_rows);
        let lines = layout
            .lines
            .iter()
            .skip(window_start)
            .take(visible_rows as usize)
            .map(|line| Line::from(Span::raw(self.text[line.start..line.end].to_string())))
            .collect::<Vec<_>>();

        frame.render_widget(block, area);
        // Keep the gutter reserved even while the scrollbar is hidden so
        // wrapping and Composer height do not jump when overflow begins.
        let content_area = Rect::new(
            inner.x,
            inner.y,
            inner.width.saturating_sub(1),
            inner.height,
        );
        frame.render_widget(Paragraph::new(lines), content_area);

        if layout.show_scrollbar {
            let mut scrollbar_state = ScrollbarState::new(layout.lines.len())
                .position(EditorViewport::scrollbar_position_for_top(
                    window_start,
                    layout.lines.len(),
                    visible_rows,
                ))
                .viewport_content_length(usize::from(visible_rows));
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .track_style(Style::default().fg(theme.scrollbar_bg))
                    .thumb_style(Style::default().fg(theme.scrollbar_fg)),
                inner,
                &mut scrollbar_state,
            );
        }
    }

    pub(crate) fn scroll_up(&mut self, width: u16, visible_rows: u16, amount: usize) {
        let layout = self.layout_for_viewport(width, visible_rows);
        self.viewport
            .scroll_up(amount, layout.lines.len(), visible_rows);
    }

    pub(crate) fn scroll_down(&mut self, width: u16, visible_rows: u16, amount: usize) {
        let layout = self.layout_for_viewport(width, visible_rows);
        self.viewport
            .scroll_down(amount, layout.lines.len(), visible_rows);
    }

    pub(crate) fn scroll_to_row(&mut self, width: u16, visible_rows: u16, row: u16) {
        let layout = self.layout_for_viewport(width, visible_rows);
        if !layout.show_scrollbar {
            return;
        }

        let track_length = usize::from(visible_rows.max(1));
        let track_row = usize::from(row.min(visible_rows.saturating_sub(1)));
        let max_top = layout.lines.len().saturating_sub(track_length);
        let top_offset = if track_length <= 1 {
            0
        } else {
            track_row.saturating_mul(max_top) / (track_length - 1)
        };
        self.viewport
            .set_top_offset(top_offset, layout.lines.len(), visible_rows);
    }

    pub(super) fn window_start(&self, layout: &EditorLayout, visible_rows: u16) -> usize {
        let visible_rows = usize::from(visible_rows.max(1));
        let top = self
            .viewport
            .top_offset(layout.lines.len(), visible_rows as u16);
        if !self.viewport.follows_cursor() {
            return top;
        }

        let cursor_index = self.cursor_visual_line_index(&layout.lines);
        let max_top = layout.lines.len().saturating_sub(visible_rows);
        if cursor_index < top {
            cursor_index
        } else if cursor_index >= top.saturating_add(visible_rows) {
            cursor_index
                .saturating_add(1)
                .saturating_sub(visible_rows)
                .min(max_top)
        } else {
            top
        }
    }

    pub(super) fn layout_for_viewport(&self, width: u16, visible_rows: u16) -> EditorLayout {
        let visible_rows = usize::from(visible_rows.max(1));
        // Reserve a one-cell gutter at every size. The scrollbar only paints
        // when these consistently wrapped lines exceed the viewport.
        let content_width = width.saturating_sub(1).max(1);
        let lines = self.visual_lines(content_width);
        EditorLayout {
            show_scrollbar: lines.len() > visible_rows,
            lines,
        }
    }

    pub(super) fn cursor_visual_line_index(&self, lines: &[std::ops::Range<usize>]) -> usize {
        lines
            .partition_point(|line| line.start <= self.cursor)
            .saturating_sub(1)
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
