//! SelectableList — shared selection kernel + row body strategies on [`Pane`] chrome.
//!
//! Feedback: Selected ≠ Active ≠ Focused
//! ([component-feedback](../../../docs/features/component-feedback.md)).
//!
//! **Kernel** owns items / selected / filter navigation.
//! **Body** is either multi-line List (`Stacked` / Settings…) or multi-column
//! Table (`Columns`) — same interaction contract, different paint.

use ratatui::{
    Frame,
    layout::Rect,
    widgets::{List, ListItem, Paragraph},
};

use crate::theme::Theme;
use crate::ui::components::feedback::{default_list_hints, empty_line};
use crate::ui::components::pane::{PaneMode, PaneSpec, PaneTitleAffix, render_pane};

mod columns;
mod panel;
mod rows;

pub use panel::{SelectablePanelBody, paint_selectable_panel};

/// How a selectable row lays out primary vs detail / value / columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectableRowLayout {
    /// Primary line + detail line underneath (menus, long descriptions).
    #[default]
    Stacked,
    /// Single line: key left-aligned, value right-aligned (`❯` selection).
    KeyValue,
    /// Multi-column single line (command palette, model picker, file browser).
    Columns,
    /// Settings catalog row: `▸ key …… value [badge] >` (selected via bg, no caret).
    SettingsRow,
    /// Settings choice option: `▸ label` (+ Active) with consequence under.
    SettingsOption,
}

/// Style role for a column cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnCellStyle {
    /// Primary label: accent when the row is selected.
    Primary,
    /// Secondary / muted description column.
    Secondary,
    /// Bold primary (named sessions); accent when selected.
    Emphasized,
    /// Status label (e.g. “active”); always accent weight.
    Status,
}

/// Horizontal alignment for a column cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColumnAlign {
    #[default]
    Left,
    Right,
}

/// One cell in a multi-column row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnCell {
    pub text: String,
    pub style: ColumnCellStyle,
    pub align: ColumnAlign,
}

impl ColumnCell {
    pub fn primary(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: ColumnCellStyle::Primary,
            align: ColumnAlign::Left,
        }
    }

    pub fn secondary(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: ColumnCellStyle::Secondary,
            align: ColumnAlign::Left,
        }
    }

    pub fn emphasized(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: ColumnCellStyle::Emphasized,
            align: ColumnAlign::Left,
        }
    }

    pub fn status(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: ColumnCellStyle::Status,
            align: ColumnAlign::Left,
        }
    }

    pub fn align(mut self, align: ColumnAlign) -> Self {
        self.align = align;
        self
    }
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

/// A single display row in a selectable list / table.
#[derive(Clone)]
pub struct SelectableItem {
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
    pub layout: SelectableRowLayout,
    /// Explicit columns for [`SelectableRowLayout::Columns`]; when empty, paint falls
    /// back to `[primary, detail]`.
    pub cells: Vec<ColumnCell>,
}

impl SelectableItem {
    pub fn new(primary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            primary: primary.into(),
            detail: detail.into(),
            is_active: false,
            badge: None,
            trailing: None,
            group: None,
            group_style: GroupHeaderStyle::Caption,
            layout: SelectableRowLayout::Stacked,
            cells: Vec::new(),
        }
    }

    /// Multi-column row. Primary/detail are filled from the first cells for filter.
    pub fn columns(cells: impl IntoIterator<Item = ColumnCell>) -> Self {
        let cells: Vec<ColumnCell> = cells.into_iter().collect();
        let primary = cells.first().map(|c| c.text.clone()).unwrap_or_default();
        let detail = cells
            .get(1..)
            .map(|rest| {
                rest.iter()
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>()
                    .join("  ")
            })
            .unwrap_or_default();
        Self {
            primary,
            detail,
            is_active: false,
            badge: None,
            trailing: None,
            group: None,
            group_style: GroupHeaderStyle::Caption,
            layout: SelectableRowLayout::Columns,
            cells,
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

    pub fn key_value(mut self) -> Self {
        self.layout = SelectableRowLayout::KeyValue;
        self
    }

    /// Settings catalog section row (`▸ label … value >`).
    pub fn settings_row(mut self) -> Self {
        self.layout = SelectableRowLayout::SettingsRow;
        self.group_style = GroupHeaderStyle::Rule;
        self
    }

    /// Settings exclusive-option row (`▸ label` + Active, detail under).
    pub fn settings_option(mut self) -> Self {
        self.layout = SelectableRowLayout::SettingsOption;
        self
    }

    /// Resolve paint cells: explicit columns or `[primary, detail]`.
    pub(super) fn resolved_cells(&self) -> Vec<ColumnCell> {
        if !self.cells.is_empty() {
            return self.cells.clone();
        }
        let mut cells = vec![ColumnCell::primary(self.primary.clone())];
        if !self.detail.is_empty() {
            cells.push(ColumnCell::secondary(self.detail.clone()));
        }
        cells
    }
}

/// Selection state for a list of items (shared by menus, sessions, settings).
pub struct SelectableList<T> {
    pub items: Vec<T>,
    pub selected: usize,
}

impl<T> Default for SelectableList<T> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl<T> SelectableList<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { items, selected: 0 }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.selected = 0;
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Select first match under `filter` (or 0 if none).
    pub fn reset_selection<F>(&mut self, filter: &str, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        let filtered = self.filtered_indices(filter, f);
        self.selected = filtered.first().copied().unwrap_or(0);
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

    pub fn selected_item(&self) -> Option<&T> {
        self.items.get(self.selected)
    }
}

/// Selectable list with **minimal** pane chrome (quick pick: agent, model, auth).
///
/// Complex browse surfaces should build a [`PaneSpec`] with
/// [`PaneMode::Standard`] and call [`render_selectable_list_with_pane`].
#[allow(clippy::too_many_arguments)]
pub fn render_selectable_list_minimal(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[SelectableItem],
    selected: usize,
    filter: &str,
    focused: bool,
    theme: &Theme,
) {
    render_selectable_list_with_mode(
        frame,
        area,
        title,
        items,
        selected,
        filter,
        focused,
        theme,
        default_list_hints(),
        PaneMode::Minimal,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_selectable_list_with_mode(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[SelectableItem],
    selected: usize,
    filter: &str,
    focused: bool,
    theme: &Theme,
    hints: &str,
    mode: PaneMode,
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

    let mut spec = match mode {
        PaneMode::Minimal => PaneSpec::minimal(title)
            .padding(PaneMode::Minimal.padding())
            .borders(PaneMode::Minimal.borders())
            .search_rule(false),
        PaneMode::Standard => PaneSpec::new(title)
            .padding(PaneMode::Standard.padding())
            .borders(PaneMode::Standard.borders()),
    };
    if filtered_count > 0 {
        spec = spec.affix(PaneTitleAffix::selection(selected_one, filtered_count));
    } else {
        spec = spec.affix(PaneTitleAffix::selection(0, 0));
    }
    spec = spec.search_filter(filter).hints(hints).focused(focused);

    paint_selectable_body(frame, area, &spec, items, selected, filter, theme);
}

/// Render a selectable list into a fully specified [`PaneSpec`].
pub fn render_selectable_list_with_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    spec: PaneSpec<'_>,
    items: &[SelectableItem],
    selected: usize,
    filter: &str,
    theme: &Theme,
) {
    paint_selectable_body(frame, area, &spec, items, selected, filter, theme);
}

fn paint_selectable_body(
    frame: &mut Frame<'_>,
    area: Rect,
    spec: &PaneSpec<'_>,
    items: &[SelectableItem],
    selected: usize,
    filter: &str,
    theme: &Theme,
) {
    let filtered: Vec<(usize, &SelectableItem)> = items
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

    let use_columns = filtered
        .first()
        .is_some_and(|(_, item)| item.layout == SelectableRowLayout::Columns);

    if use_columns {
        let col_items: Vec<&SelectableItem> = filtered.iter().map(|(_, item)| *item).collect();
        columns::paint_column_items(frame, content, &col_items, selected_filtered_idx, theme);
        return;
    }

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

fn item_matches_filter(item: &SelectableItem, filter: &str) -> bool {
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
        || item
            .cells
            .iter()
            .any(|c| c.text.to_lowercase().contains(&f))
}
