pub mod approval;
pub mod commands;
pub mod help;
pub mod models;
pub mod sessions;
pub mod settings;
pub mod status;
pub mod status_panel;
pub mod timeline;
pub mod tree;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

/// Shared list-item rendering for two-line selector rows.
pub fn selector_item<'a>(selected: bool, primary: String, detail: String) -> ListItem<'a> {
    let marker = if selected { "> " } else { "  " };
    let style = if selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    ListItem::new(vec![
        Line::from(Span::styled(format!("{marker}{primary}"), style)),
        Line::from(Span::styled(
            format!("  {detail}"),
            Style::default().fg(Color::DarkGray),
        )),
    ])
}

/// Center a `percent_x × percent_y` popup inside `area`.
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

pub fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

pub fn preview_text(text: &str) -> String {
    let mut out = text.chars().take(96).collect::<String>();
    if text.chars().count() > 96 {
        out.push_str("...");
    }
    out
}
