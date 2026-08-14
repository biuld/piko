//! Ratatui paint for the Todos dock strip within a granted rect.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use piko_protocol::{TodoList, TodoStatus};

use super::project::{max_item_rows_for_grant, project_strip};
use crate::theme::Theme;

pub fn paint_strip(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    list: &TodoList,
    collapsed: bool,
    theme: &Theme,
    header_hovered: bool,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let max_items = max_item_rows_for_grant(area.height, list.items.len());
    let view = project_strip(list, area.width, max_items, collapsed);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let header_style = Style::default()
        .fg(if header_hovered {
            theme.accent
        } else {
            theme.text
        })
        .add_modifier(Modifier::BOLD);
    lines.push(Line::from(Span::styled(view.header, header_style)));

    for row in view.rows {
        let (mark_style, content_style) = status_styles(row.status, theme);
        let mut spans = vec![
            Span::styled(format!("{} ", row.mark), mark_style),
            Span::styled(row.content, content_style),
        ];
        // Strikethrough content only for completed.
        if row.status == TodoStatus::Completed {
            spans[1] = Span::styled(
                spans[1].content.to_string(),
                content_style.add_modifier(Modifier::CROSSED_OUT),
            );
        }
        lines.push(Line::from(spans));
    }

    if let Some(overflow) = view.overflow
        && (lines.len() as u16) < area.height
    {
        lines.push(Line::from(Span::styled(
            overflow,
            Style::default().fg(theme.dim),
        )));
    }

    // Never paint more rows than granted.
    lines.truncate(area.height as usize);
    frame.render_widget(Paragraph::new(lines), area);
}

fn status_styles(status: TodoStatus, theme: &Theme) -> (Style, Style) {
    match status {
        TodoStatus::Completed => (
            Style::default().fg(theme.dim),
            Style::default().fg(theme.dim),
        ),
        TodoStatus::InProgress => (
            Style::default().fg(theme.warning),
            Style::default().fg(theme.text),
        ),
        TodoStatus::Pending => (
            Style::default().fg(theme.dim),
            Style::default().fg(theme.dim),
        ),
    }
}
