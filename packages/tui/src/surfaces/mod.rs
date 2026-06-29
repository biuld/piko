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
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

/// Generic list selection state manager.
pub struct SelectorList<T> {
    pub items: Vec<T>,
    pub selected: usize,
}

impl<T> SelectorList<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { items, selected: 0 }
    }

    pub fn filtered_indices<F>(&self, filter: &str, mut f: F) -> Vec<usize>
    where
        F: FnMut(&T) -> bool,
    {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if filter.is_empty() {
                    true
                } else {
                    f(item)
                }
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    pub fn select_next<F>(&mut self, filter: &str, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        let filtered = self.filtered_indices(filter, f);
        if filtered.is_empty() {
            return;
        }
        let current_filtered_pos = filtered
            .iter()
            .position(|&orig_idx| orig_idx == self.selected)
            .unwrap_or(0);
        let next_filtered_pos = (current_filtered_pos + 1).min(filtered.len() - 1);
        if let Some(&orig_idx) = filtered.get(next_filtered_pos) {
            self.selected = orig_idx;
        }
    }

    pub fn select_prev<F>(&mut self, filter: &str, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        let filtered = self.filtered_indices(filter, f);
        if filtered.is_empty() {
            return;
        }
        let current_filtered_pos = filtered
            .iter()
            .position(|&orig_idx| orig_idx == self.selected)
            .unwrap_or(0);
        let prev_filtered_pos = current_filtered_pos.saturating_sub(1);
        if let Some(&orig_idx) = filtered.get(prev_filtered_pos) {
            self.selected = orig_idx;
        }
    }
}

#[derive(Clone)]
pub struct SelectItem {
    pub primary: String,
    pub detail: String,
    pub is_active: bool,
}

pub struct SelectListView;

impl SelectListView {
    pub fn render(
        frame: &mut Frame<'_>,
        area: Rect,
        title: &str,
        items: &[SelectItem],
        selected: usize,
        filter: &str,
    ) {
        let filtered: Vec<(usize, &SelectItem)> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                if filter.is_empty() {
                    true
                } else {
                    let f = filter.to_lowercase();
                    item.primary.to_lowercase().contains(&f) || item.detail.to_lowercase().contains(&f)
                }
            })
            .collect();

        use ratatui::widgets::Clear;
        frame.render_widget(Clear, area);

        if filtered.is_empty() {
            let body = if filter.is_empty() {
                "No items available."
            } else {
                "No items match the filter."
            };
            let block = Block::default().borders(Borders::ALL).title(title);
            let widget = Paragraph::new(body).block(block);
            frame.render_widget(widget, area);
            return;
        }

        let selected_filtered_idx = filtered
            .iter()
            .position(|&(orig_idx, _)| orig_idx == selected)
            .unwrap_or(0)
            .min(filtered.len().saturating_sub(1));

        let list_items: Vec<ListItem<'_>> = filtered
            .iter()
            .enumerate()
            .map(|(idx, &(_, item))| {
                let marker = if idx == selected_filtered_idx { "> " } else { "  " };
                let style = if idx == selected_filtered_idx {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let active_suffix = if item.is_active { " *" } else { "" };
                let primary_disp = if item.primary.len() > 60 {
                    let (left, right) = item.primary.split_at(30);
                    format!("{}...{}{}", left, &right[right.len() - 27..], active_suffix)
                } else {
                    format!("{}{}", item.primary, active_suffix)
                };

                let detail_disp = if item.detail.len() > area.width.saturating_sub(10) as usize {
                    let mut d = item.detail.chars().take(area.width.saturating_sub(13) as usize).collect::<String>();
                    d.push_str("...");
                    d
                } else {
                    item.detail.clone()
                };

                ListItem::new(vec![
                    Line::from(Span::styled(format!("{marker}{primary_disp}"), style)),
                    Line::from(Span::styled(
                        format!("  {detail_disp}"),
                        Style::default().fg(Color::DarkGray),
                    )),
                ])
            })
            .collect();

        let filter_part = if filter.is_empty() {
            "".to_string()
        } else {
            format!(" | filter: {}", filter)
        };
        let counter_part = format!(" [{}/{}]", selected_filtered_idx + 1, filtered.len());
        let full_title = format!("{} {}{} | Enter confirm | Esc close", title, filter_part, counter_part);

        let list = List::new(list_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(full_title),
        );

        let mut list_state = ratatui::widgets::ListState::default();
        list_state.select(Some(selected_filtered_idx));
        frame.render_stateful_widget(list, area, &mut list_state);
    }
}

use ratatui::Frame;

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
