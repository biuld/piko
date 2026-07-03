//! FilterableList — reusable component for overlays with keyboard-navigable items.
//!
//! Used by: CommandPalette, ModelSelector, SessionList, SettingsPanel, TreePanel, etc.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use ratatui::widgets::Clear;

use crate::theme::Theme;

/// A single display row in a filterable list.
#[derive(Clone)]
pub struct FilterableItem {
    pub primary: String,
    pub detail: String,
    pub is_active: bool,
}

/// Selection state for a list of items.
pub struct FilterableList<T> {
    pub items: Vec<T>,
    pub selected: usize,
}

impl<T> FilterableList<T> {
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
            .filter(|(_, item)| if filter.is_empty() { true } else { f(item) })
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

/// Renders a filterable list with keyboard navigation markers.
pub fn render_filterable_list(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[FilterableItem],
    selected: usize,
    filter: &str,
    theme: &Theme,
) {
    let filtered: Vec<(usize, &FilterableItem)> = items
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

    frame.render_widget(Clear, area);

    if filtered.is_empty() {
        let body = if filter.is_empty() {
            "No items available."
        } else {
            "No items match the filter."
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_muted))
            .title(title);
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
            let marker = if idx == selected_filtered_idx {
                "> "
            } else {
                "  "
            };
            let style = if idx == selected_filtered_idx {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let active_suffix = if item.is_active { " *" } else { "" };
            let primary_disp = format!(
                "{}{}",
                middle_elide_chars(&item.primary, 60, 30, 27),
                active_suffix
            );

            let detail_disp = if item.detail.len() > area.width.saturating_sub(10) as usize {
                let mut d = item
                    .detail
                    .chars()
                    .take(area.width.saturating_sub(13) as usize)
                    .collect::<String>();
                d.push_str("...");
                d
            } else {
                item.detail.clone()
            };

            ListItem::new(vec![
                Line::from(Span::styled(format!("{marker}{primary_disp}"), style)),
                Line::from(Span::styled(
                    format!("  {detail_disp}"),
                    Style::default().fg(theme.dim),
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
    let full_title = format!(
        "{} {}{} | Enter confirm | Esc close",
        title, filter_part, counter_part
    );

    let list = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_muted))
            .title(full_title),
    );

    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(selected_filtered_idx));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn middle_elide_chars(
    text: &str,
    max_chars: usize,
    head_chars: usize,
    tail_chars: usize,
) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let head = text.chars().take(head_chars).collect::<String>();
    let tail = text
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}...{tail}")
}

#[cfg(test)]
mod tests {
    use super::middle_elide_chars;

    #[test]
    fn middle_elide_handles_multibyte_text() {
        let text = "这是一个包含很多中文字符的会话树条目，用来验证截断不会落在字符边界中间导致崩溃";
        let elided = middle_elide_chars(text, 20, 10, 8);

        assert!(elided.contains("..."));
        assert!(elided.starts_with("这是一个包含很多中"));
        assert!(elided.ends_with("边界中间导致崩溃"));
    }
}
