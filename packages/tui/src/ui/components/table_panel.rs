#![allow(clippy::type_complexity, clippy::large_enum_variant)]

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Row, Table, TableState},
};
use crate::theme::Theme;

/// ActionPrompt represents the footer area content of the table panel.
pub enum ActionPrompt<'a> {
    /// A single line of help/legend text
    Legend(&'a str),
    /// A custom interactive sub-panel, e.g. for user input (like SummaryPrompt)
    Interactive {
        height: u16,
        render: Box<dyn FnOnce(&mut Frame<'_>, Rect) + 'a>,
    },
}

/// TableBody represents the main scrollable content area.
pub enum TableBody<'a> {
    /// Render standard table rows
    Rows {
        widths: &'a [Constraint],
        rows: Vec<Row<'a>>,
        selected_idx: usize,
    },
    /// Render placeholder/status message
    Message(Paragraph<'a>),
}

/// A high-level container component for panels displaying searchable lists/tables.
pub struct TablePanel<'a> {
    pub left_title: String,
    pub mode_indicator: String,
    pub counter: String,
    pub help_text: &'a str,
    pub search_line: Line<'a>,
    pub body: TableBody<'a>,
    pub action_prompt: ActionPrompt<'a>,
    pub gap: bool,
}

impl<'a> TablePanel<'a> {
    pub fn render(self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        frame.render_widget(Clear, area);

        let right_title = format!("{}  {}", self.mode_indicator, self.counter);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(Line::from(self.left_title).alignment(Alignment::Left))
            .title(Line::from(right_title).alignment(Alignment::Right));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let footer_height = match &self.action_prompt {
            ActionPrompt::Legend(_) => 1,
            ActionPrompt::Interactive { height, .. } => *height,
        };

        let gap_size = if self.gap { 1 } else { 0 };
        let min_inner_height = 4 + gap_size + 1 + footer_height;
        if inner.height < min_inner_height {
            return;
        }

        // 1. Render Help Text (row 0 of inner)
        frame.render_widget(
            Paragraph::new(Line::from(ratatui::text::Span::styled(
                self.help_text,
                Style::default().fg(theme.muted),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        // 2. Render Search/Input line (row 2 of inner, row 1 is empty)
        frame.render_widget(
            Paragraph::new(self.search_line),
            Rect::new(inner.x, inner.y + 2, inner.width, 1),
        );

        // 3. Compute Areas
        let content_height = inner.height - 4 - footer_height - gap_size;
        let content_area = Rect::new(inner.x, inner.y + 4, inner.width, content_height);

        let footer_area = Rect::new(
            inner.x,
            inner.y + inner.height - footer_height,
            inner.width,
            footer_height,
        );

        // 4. Render Body
        match self.body {
            TableBody::Message(p) => {
                frame.render_widget(p, content_area);
            }
            TableBody::Rows {
                widths,
                rows,
                selected_idx,
            } => {
                let mut table_state = TableState::default().with_selected(Some(selected_idx));
                let table = Table::new(rows, widths).row_highlight_style(Style::default());
                frame.render_stateful_widget(table, content_area, &mut table_state);
            }
        }

        // 5. Render Action Prompt (Footer)
        match self.action_prompt {
            ActionPrompt::Legend(txt) => {
                let p = Paragraph::new(Line::from(ratatui::text::Span::styled(
                    txt,
                    Style::default().fg(theme.muted),
                )));
                frame.render_widget(p, footer_area);
            }
            ActionPrompt::Interactive { render, .. } => {
                render(frame, footer_area);
            }
        }
    }
}
