//! Ratatui paint for the centered Todos overlay content.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use piko_protocol::{TodoList, TodoStatus};

use super::project::{max_item_rows_for_grant, project_strip};
use crate::theme::Theme;

pub fn paint_overlay(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    list: &TodoList,
    scroll: usize,
    theme: &Theme,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let max_items = max_item_rows_for_grant(area.height, list.items.len());
    let mut view = project_strip(list, area.width, max_items, false, scroll);
    view.header = view.header.trim_start_matches("▾ ").to_string();
    let mut lines = vec![Line::from(Span::styled(
        view.header,
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    ))];
    for row in view.rows {
        let (mark_style, mut content_style) = status_styles(row.status, theme);
        if row.status == TodoStatus::Completed {
            content_style = content_style.add_modifier(Modifier::CROSSED_OUT);
        }
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", row.mark), mark_style),
            Span::styled(row.content, content_style),
        ]));
    }
    if let Some(hint) = view.scroll_hint {
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(theme.dim),
        )));
    }
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
