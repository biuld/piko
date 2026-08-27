use super::*;
use crate::theme::Theme;
use crate::ui::text_layout::{Breakability, TextLayout, TextRun, wrap_runs};
use ratatui::style::Style;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

pub(super) struct EditorLayout {
    pub(super) text: TextLayout<()>,
    pub(super) lines: Vec<std::ops::Range<usize>>,
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
        let position = layout.text.visual_position(self.cursor);
        let index = position.row.min(layout.lines.len().saturating_sub(1));
        if layout.lines.get(index).is_none() {
            return (0, 0);
        }
        let window_start = self.window_start(&layout, visible_rows);
        (index.saturating_sub(window_start) as u16, position.col)
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
        let viewport = self
            .viewport
            .prepare(inner, layout.lines.len(), visible_rows, window_start);
        let visible = viewport.visible.clone();
        let lines = layout
            .text
            .lines
            .iter()
            .skip(visible.start)
            .take(visible.len())
            .map(|line| {
                Line::from(
                    line.fragments
                        .iter()
                        .map(|fragment| Span::raw(fragment.text.clone()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();

        frame.render_widget(block, area);
        // Keep the gutter reserved even while the scrollbar is hidden so
        // wrapping and Composer height do not jump when overflow begins.
        frame.render_widget(Paragraph::new(lines), viewport.content);

        if let Some(metrics) = viewport.scrollbar {
            let mut scrollbar_state = ScrollbarState::new(layout.lines.len())
                .position(metrics.content_position())
                .viewport_content_length(metrics.visible_rows.max(1));
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .track_style(Style::default().fg(theme.scrollbar_bg))
                    .thumb_style(Style::default().fg(theme.scrollbar_fg)),
                viewport.gutter,
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
        let visible_rows = visible_rows.max(1);
        let current_top = self.viewport.top_offset(layout.lines.len(), visible_rows);
        let plan = self.viewport.prepare(
            Rect::new(0, 0, width, visible_rows),
            layout.lines.len(),
            visible_rows,
            current_top,
        );
        let Some(metrics) = plan.scrollbar else {
            return;
        };
        let top_offset = metrics.top_for_track_row(row);
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
        let _visible_rows = visible_rows.max(1);
        // Reserve a one-cell gutter at every size. The scrollbar only paints
        // when these consistently wrapped lines exceed the viewport.
        let content_width = width.saturating_sub(1).max(1);
        let text = self.text_layout(content_width);
        let lines: Vec<std::ops::Range<usize>> = (0..text.lines.len())
            .filter_map(|row| text.line_source_range(row))
            .collect();
        EditorLayout { text, lines }
    }

    pub(super) fn cursor_visual_line_index(&self, lines: &[std::ops::Range<usize>]) -> usize {
        lines
            .partition_point(|line| line.start <= self.cursor)
            .saturating_sub(1)
    }

    /// Prepare the editor's source-aware visual layout.  Reference
    /// placeholders are atomic runs, while ordinary text wraps at grapheme
    /// boundaries; the resulting plan is also used by pointer placement.
    pub(super) fn text_layout(&self, width: u16) -> TextLayout<()> {
        let mut runs = Vec::new();
        let mut cursor = 0usize;
        for (start, end) in self.reference_ranges() {
            if start > cursor {
                runs.push(
                    TextRun::new(
                        self.text[cursor..start].to_string(),
                        (),
                        Breakability::Grapheme,
                    )
                    .with_source(cursor..start),
                );
            }
            if end > start {
                runs.push(
                    TextRun::new(self.text[start..end].to_string(), (), Breakability::Atomic)
                        .with_source(start..end),
                );
            }
            cursor = end;
        }
        if cursor < self.text.len() || runs.is_empty() {
            runs.push(
                TextRun::new(self.text[cursor..].to_string(), (), Breakability::Grapheme)
                    .with_source(cursor..self.text.len()),
            );
        }
        wrap_runs(runs, usize::from(width.max(1)))
    }
}
