//! FilterableList — keyboard list state + row paint on shared [`Pane`] chrome.
//!
//! Feedback: Selected ≠ Active ≠ Focused ([component-feedback](../../../docs/features/component-feedback.md)).

use ratatui::{
    Frame,
    layout::Rect,
    widgets::{List, ListItem, Paragraph},
};

use crate::theme::Theme;
use crate::ui::components::feedback::{default_list_hints, empty_line, list_title};
use crate::ui::components::pane::{PaneSpec, render_pane};

mod rows;

/// How a filterable row lays out primary vs detail / value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilterableRowLayout {
    /// Primary line + detail line underneath (menus, long descriptions).
    #[default]
    Stacked,
    /// Single line: key left-aligned, value right-aligned (`❯` selection).
    #[allow(dead_code)] // reserved for key/value menus
    KeyValue,
    /// Settings catalog row: `▸ key …… value [badge] >` (selected via bg, no caret).
    SettingsRow,
    /// Settings choice option: `▸ label` (+ Active) with consequence under.
    SettingsOption,
}

/// How a domain group caption is painted above the first visible row of a chunk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupHeaderStyle {
    /// Dim bold label (menus / generic lists).
    #[default]
    Caption,
    /// `Name ────────` rule (Settings catalog).
    Rule,
}

/// Leading bullet for Settings-style rows.
pub(super) const SETTINGS_BULLET: &str = "▸ ";
/// Drill / expand affix after a Settings catalog value (screenshot chevron).
pub(super) const SETTINGS_EXPAND: &str = " ›";

/// A single display row in a filterable list.
#[derive(Clone)]
pub struct FilterableItem {
    pub primary: String,
    pub detail: String,
    /// Authoritative "already in force" value (not keyboard selection).
    pub is_active: bool,
    /// Optional affix painted with `warning` (effect class, etc.).
    pub badge: Option<String>,
    /// Trailing primary affix (e.g. drill `▸` for menus); dim, non-active.
    pub trailing: Option<String>,
    /// Domain chunk name: filter match + non-selectable header when group changes.
    pub group: Option<String>,
    pub group_style: GroupHeaderStyle,
    pub layout: FilterableRowLayout,
}

impl FilterableItem {
    pub fn new(primary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            primary: primary.into(),
            detail: detail.into(),
            is_active: false,
            badge: None,
            trailing: None,
            group: None,
            group_style: GroupHeaderStyle::Caption,
            layout: FilterableRowLayout::Stacked,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }

    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn trailing(mut self, trailing: impl Into<String>) -> Self {
        self.trailing = Some(trailing.into());
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn group_rule(mut self) -> Self {
        self.group_style = GroupHeaderStyle::Rule;
        self
    }

    #[allow(dead_code)] // reserved for key/value menus
    pub fn key_value(mut self) -> Self {
        self.layout = FilterableRowLayout::KeyValue;
        self
    }

    /// Settings catalog section row (`▸ label … value >`).
    pub fn settings_row(mut self) -> Self {
        self.layout = FilterableRowLayout::SettingsRow;
        self.group_style = GroupHeaderStyle::Rule;
        self
    }

    /// Settings exclusive-option row (`▸ label` + Active, detail under).
    pub fn settings_option(mut self) -> Self {
        self.layout = FilterableRowLayout::SettingsOption;
        self
    }
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
    render_filterable_list_with_hints(
        frame,
        area,
        title,
        items,
        selected,
        filter,
        focused,
        theme,
        default_list_hints(),
    );
}

/// Like [`render_filterable_list`] with a custom footer hint line.
#[allow(clippy::too_many_arguments)]
pub fn render_filterable_list_with_hints(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[FilterableItem],
    selected: usize,
    filter: &str,
    focused: bool,
    theme: &Theme,
    hints: &str,
) {
    let filtered_count = items
        .iter()
        .filter(|item| item_matches_filter(item, filter))
        .count();
    let selected_one = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item_matches_filter(item, filter))
        .position(|(orig, _)| orig == selected)
        .map(|i| i + 1)
        .unwrap_or(0);

    let full_title = list_title(title, filter, selected_one, filtered_count);
    let spec = PaneSpec::new(&full_title)
        .search_filter(filter)
        .hints(hints)
        .focused(focused);

    paint_list_body(frame, area, &spec, items, selected, filter, theme);
}

/// Render a filterable list into a fully specified [`PaneSpec`].
///
/// Title is used as-is (no `[n/total]` rewrite) — for Settings-style product chrome.
pub fn render_filterable_list_with_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    spec: PaneSpec<'_>,
    items: &[FilterableItem],
    selected: usize,
    filter: &str,
    theme: &Theme,
) {
    paint_list_body(frame, area, &spec, items, selected, filter, theme);
}

fn paint_list_body(
    frame: &mut Frame<'_>,
    area: Rect,
    spec: &PaneSpec<'_>,
    items: &[FilterableItem],
    selected: usize,
    filter: &str,
    theme: &Theme,
) {
    let filtered: Vec<(usize, &FilterableItem)> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item_matches_filter(item, filter))
        .collect();

    let Some(areas) = render_pane(frame, area, spec, theme) else {
        return;
    };

    let content = areas.content;
    if filtered.is_empty() {
        frame.render_widget(
            Paragraph::new(empty_line(!filter.is_empty(), theme)),
            content,
        );
        return;
    }

    let selected_filtered_idx = filtered
        .iter()
        .position(|&(orig_idx, _)| orig_idx == selected)
        .unwrap_or(0)
        .min(filtered.len().saturating_sub(1));

    let row_width = content.width.max(1) as usize;
    let list_items: Vec<ListItem<'_>> = filtered
        .iter()
        .enumerate()
        .map(|(idx, &(_, item))| {
            let is_selected = idx == selected_filtered_idx;
            let prev = filtered.get(idx.wrapping_sub(1)).map(|(_, p)| *p);
            let mut lines = rows::leading_group_lines(item, idx > 0, prev, row_width, theme);
            lines.extend(rows::row_lines(item, is_selected, row_width, theme));
            ListItem::new(lines)
        })
        .collect();

    let list = List::new(list_items);
    let mut list_state = ratatui::widgets::ListState::default();
    list_state.select(Some(selected_filtered_idx));
    frame.render_stateful_widget(list, content, &mut list_state);
}

fn item_matches_filter(item: &FilterableItem, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let f = filter.to_lowercase();
    item.primary.to_lowercase().contains(&f)
        || item.detail.to_lowercase().contains(&f)
        || item
            .badge
            .as_ref()
            .is_some_and(|b| b.to_lowercase().contains(&f))
        || item
            .group
            .as_ref()
            .is_some_and(|g| g.to_lowercase().contains(&f))
}
