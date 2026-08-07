//! FilterableList — reusable component for overlays with keyboard-navigable items.
//!
//! Feedback contract: [component-feedback](../../../docs/features/component-feedback.md)
//! Selected (`❯` + accent) ≠ Active (`●`) ≠ Focused (`borderAccent`).

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

use crate::theme::Theme;
use crate::ui::components::feedback::{
    active_marker_span, default_list_hints, empty_line, frame_border_style, hint_line, list_title,
    row_detail_style, row_primary_style, selection_prefix, with_selected_bg,
};

/// A single display row in a filterable list.
#[derive(Clone)]
pub struct FilterableItem {
    pub primary: String,
    pub detail: String,
    /// Authoritative "already in force" value (not keyboard selection).
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

/// Renders a filterable list with component-feedback selection language.
///
/// `focused`: when true, frame uses accent border (surface owns keyboard).
#[allow(clippy::too_many_arguments)]
pub fn render_filterable_list(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[FilterableItem],
    selected: usize,
    filter: &str,
    focused: bool,
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

    let border = frame_border_style(focused, theme);

    if filtered.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(list_title(title, filter, 0, 0));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.height == 0 {
            return;
        }
        let body = Paragraph::new(vec![
            empty_line(!filter.is_empty(), theme),
            Line::default(),
            hint_line(default_list_hints(), theme),
        ]);
        frame.render_widget(body, inner);
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
            let is_selected = idx == selected_filtered_idx;
            let marker = selection_prefix(is_selected);
            let mut primary_style =
                with_selected_bg(row_primary_style(is_selected, theme), is_selected, theme);
            // Non-selected active rows keep default text; active marker carries the cue.
            if !is_selected && item.is_active {
                primary_style = primary_style.fg(theme.text);
            }

            let primary_disp = middle_elide_chars(&item.primary, 60, 30, 27);
            let mut primary_spans = vec![
                Span::styled(
                    marker,
                    if is_selected {
                        Style::default().fg(theme.accent)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(primary_disp, primary_style),
            ];
            if item.is_active {
                primary_spans.push(active_marker_span(theme));
            }

            let detail_disp =
                if item.detail.chars().count() > area.width.saturating_sub(10) as usize {
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
                Line::from(primary_spans),
                Line::from(Span::styled(
                    format!("  {detail_disp}"),
                    row_detail_style(theme),
                )),
            ])
        })
        .collect();

    let full_title = list_title(title, filter, selected_filtered_idx + 1, filtered.len());

    // Reserve last line of the block for dim key hints when height allows.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(full_title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let (list_area, hint_area) = if inner.height >= 3 {
        (
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: inner.height.saturating_sub(1),
            },
            Some(Rect {
                x: inner.x,
                y: inner.y + inner.height.saturating_sub(1),
                width: inner.width,
                height: 1,
            }),
        )
    } else {
        (inner, None)
    };

    let list = List::new(list_items);
    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(selected_filtered_idx));
    frame.render_stateful_widget(list, list_area, &mut list_state);

    if let Some(hint_area) = hint_area {
        frame.render_widget(
            Paragraph::new(hint_line(default_list_hints(), theme)),
            hint_area,
        );
    }
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
