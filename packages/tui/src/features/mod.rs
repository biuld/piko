pub mod agent_list;
pub mod agent_status;
pub mod approval;
pub mod auth_selector;
pub mod auto_completion;
pub mod bottom_bar;
pub mod editor;
pub mod help;
pub mod model_selector;
pub mod notifications;
pub mod session_list;
pub mod settings;
pub mod status;
pub mod timeline;
pub mod tool_interaction;
pub mod tree;

use ratatui::layout::Rect;

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
