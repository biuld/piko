//! MenuStack — generic drill-down menu (Auth selector, Settings catalog).
//!
//! Owns the navigation stack, filter-aware selection (delegated to
//! [`SelectableList`]), and the row → [`SelectableItem`] mapping. Product chrome
//! stays in the consumer: Auth renders a minimal band, Settings a Standard
//! pane with value summaries and effect badges.

use ratatui::{Frame, layout::Rect};

use crate::{
    theme::Theme,
    ui::components::{
        GROUP_DRILL,
        selectable_list::{SelectableItem, SelectableList, render_selectable_list_minimal},
    },
};

/// How rows in a frame are painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuRowLayout {
    /// Generic two-line rows (auth selector).
    Stacked,
    /// Settings catalog row: `▸ key …… value [badge] ›`.
    SettingsRow,
    /// Settings exclusive-option row: `▸ label` + Active, consequence under.
    SettingsOption,
}

/// What confirming a row does.
#[derive(Clone, Debug)]
pub enum MenuRowKind<T: Clone> {
    /// Drill into a sub-frame with the same row layout.
    Branch(Vec<MenuRow<T>>),
    /// Drill into a settings choice page (option rows, custom frame title).
    Choice {
        title: String,
        options: Vec<MenuRow<T>>,
    },
    /// Leaf action; confirming returns it.
    Action(T),
}

/// One display row in any menu frame.
#[derive(Clone, Debug)]
pub struct MenuRow<T: Clone> {
    pub title: String,
    pub detail: String,
    /// Settings ValueSummary (painted on SettingsRow; filter-matched too).
    pub value: Option<String>,
    /// Warning badge text, e.g. "restart hostd".
    pub badge: Option<String>,
    /// Domain chunk header (settings root).
    pub group: Option<String>,
    /// Authoritative in-force value (not keyboard selection).
    pub is_active: bool,
    pub kind: MenuRowKind<T>,
}

impl<T: Clone> MenuRow<T> {
    /// Plain leaf row (auth action or settings option).
    pub fn action(title: impl Into<String>, detail: impl Into<String>, action: T) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            value: None,
            badge: None,
            group: None,
            is_active: false,
            kind: MenuRowKind::Action(action),
        }
    }

    /// Map this row onto a shared [`SelectableItem`] for the frame's layout.
    pub fn to_item(&self, layout: MenuRowLayout) -> SelectableItem {
        match layout {
            MenuRowLayout::Stacked => {
                let mut row = SelectableItem::new(self.title.clone(), self.detail.clone())
                    .active(self.is_active);
                if matches!(self.kind, MenuRowKind::Branch(_)) {
                    row = row.trailing(GROUP_DRILL);
                }
                row
            }
            MenuRowLayout::SettingsRow => {
                let mut row =
                    SelectableItem::new(self.title.clone(), self.value.clone().unwrap_or_default())
                        .settings_row();
                if let Some(badge) = &self.badge {
                    row = row.badge(badge.clone());
                }
                if let Some(group) = &self.group {
                    row = row.with_group(group.clone()).group_rule();
                }
                row
            }
            MenuRowLayout::SettingsOption => {
                let mut row = SelectableItem::new(self.title.clone(), self.detail.clone())
                    .settings_option()
                    .active(self.is_active);
                if let Some(badge) = &self.badge {
                    row = row.badge(badge.clone());
                }
                row
            }
        }
    }
}

/// One level of the navigation stack.
pub struct MenuFrame<T: Clone> {
    pub title: String,
    pub layout: MenuRowLayout,
    pub items: SelectableList<MenuRow<T>>,
}

/// Result of confirming a row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuConfirmResult<T> {
    /// Entered a sub-frame (branch or choice page).
    Drilled,
    /// Leaf action was confirmed.
    Apply(T),
    None,
}

/// Filter-aware drill-down navigation stack.
pub struct MenuStack<T: Clone> {
    frames: Vec<MenuFrame<T>>,
}

impl<T: Clone> MenuStack<T> {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    /// Reset the stack to a single root frame.
    pub fn open(&mut self, title: impl Into<String>, layout: MenuRowLayout, rows: Vec<MenuRow<T>>) {
        self.frames.clear();
        self.push(title, layout, rows);
    }

    fn push(&mut self, title: impl Into<String>, layout: MenuRowLayout, rows: Vec<MenuRow<T>>) {
        self.frames.push(MenuFrame {
            title: title.into(),
            layout,
            items: SelectableList::new(rows),
        });
    }

    pub fn current(&self) -> Option<&MenuFrame<T>> {
        self.frames.last()
    }

    pub fn at_root(&self) -> bool {
        self.frames.len() <= 1
    }

    /// Pop the top frame; returns true while frames remain.
    pub fn pop(&mut self) -> bool {
        self.frames.pop();
        !self.frames.is_empty()
    }

    pub fn select_next(&mut self, filter: &str) {
        if let Some(frame) = self.frames.last_mut() {
            frame.items.select_next(filter, |r| row_matches(r, filter));
        }
    }

    pub fn select_prev(&mut self, filter: &str) {
        if let Some(frame) = self.frames.last_mut() {
            frame.items.select_prev(filter, |r| row_matches(r, filter));
        }
    }

    pub fn reset_selection(&mut self) {
        if let Some(frame) = self.frames.last_mut() {
            frame.items.selected = 0;
        }
    }

    /// Filtered item count at the current frame (for Select band budgets).
    pub fn filtered_item_count(&self, filter: &str) -> usize {
        self.frames
            .last()
            .map(|f| {
                f.items
                    .filtered_indices(filter, |r| row_matches(r, filter))
                    .len()
            })
            .unwrap_or(0)
    }

    /// Confirm the currently selected row: drill, or return the leaf action.
    pub fn confirm(&mut self, filter: &mut String) -> MenuConfirmResult<T> {
        let (pending, parent_layout) = {
            let Some(frame) = self.frames.last() else {
                return MenuConfirmResult::None;
            };
            let filter_str = filter.as_str();
            let filtered = frame
                .items
                .filtered_indices(filter_str, |r| row_matches(r, filter_str));
            if filtered.is_empty() {
                return MenuConfirmResult::None;
            }
            let pos = filtered
                .iter()
                .position(|&i| i == frame.items.selected)
                .unwrap_or(0)
                .min(filtered.len() - 1);
            let Some(&idx) = filtered.get(pos) else {
                return MenuConfirmResult::None;
            };
            (frame.items.items[idx].clone(), frame.layout)
        };

        match pending.kind {
            MenuRowKind::Branch(children) => {
                self.push(pending.title, parent_layout, children);
                filter.clear();
                MenuConfirmResult::Drilled
            }
            MenuRowKind::Choice { title, options } => {
                self.push(title, MenuRowLayout::SettingsOption, options);
                filter.clear();
                MenuConfirmResult::Drilled
            }
            MenuRowKind::Action(action) => MenuConfirmResult::Apply(action),
        }
    }

    /// Render the current frame with minimal (quick-pick) chrome.
    pub fn render_minimal(&self, frame: &mut Frame<'_>, area: Rect, filter: &str, theme: &Theme) {
        let Some(current) = self.frames.last() else {
            return;
        };
        let items: Vec<SelectableItem> = current
            .items
            .items
            .iter()
            .map(|r| r.to_item(current.layout))
            .collect();
        render_selectable_list_minimal(
            frame,
            area,
            &current.title,
            &items,
            current.items.selected,
            filter,
            true,
            theme,
        );
    }
}

impl<T: Clone> Default for MenuStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

fn row_matches<T: Clone>(row: &MenuRow<T>, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let f = filter.to_lowercase();
    row.title.to_lowercase().contains(&f)
        || row.detail.to_lowercase().contains(&f)
        || row
            .value
            .as_deref()
            .is_some_and(|v| v.to_lowercase().contains(&f))
        || row
            .badge
            .as_deref()
            .is_some_and(|b| b.to_lowercase().contains(&f))
        || row
            .group
            .as_deref()
            .is_some_and(|g| g.to_lowercase().contains(&f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> Vec<MenuRow<&'static str>> {
        vec![
            MenuRow {
                title: "Retries".into(),
                detail: String::new(),
                value: Some("On".into()),
                badge: None,
                group: Some("Runtime".into()),
                is_active: false,
                kind: MenuRowKind::Choice {
                    title: "API Retries".into(),
                    options: vec![
                        MenuRow::action("On", "retry", "on"),
                        MenuRow::action("Off", "no", "off"),
                    ],
                },
            },
            MenuRow::action("Quit", "exit", "quit"),
        ]
    }

    #[test]
    fn drill_branch_then_apply() {
        let mut stack = MenuStack::new();
        stack.open("settings", MenuRowLayout::SettingsRow, root());
        assert!(stack.at_root());
        assert!(matches!(
            stack.confirm(&mut String::new()),
            MenuConfirmResult::Drilled
        ));
        assert!(!stack.at_root());
        assert_eq!(
            stack.current().map(|f| f.layout),
            Some(MenuRowLayout::SettingsOption)
        );
        stack.select_next("");
        assert_eq!(
            stack.confirm(&mut String::new()),
            MenuConfirmResult::Apply("off")
        );
    }

    #[test]
    fn filter_matches_value_and_badge() {
        let mut stack = MenuStack::new();
        stack.open("settings", MenuRowLayout::SettingsRow, root());
        assert_eq!(stack.filtered_item_count("Runtime"), 1);
        assert_eq!(stack.filtered_item_count("On"), 1);
    }

    #[test]
    fn action_returns_directly() {
        let mut stack = MenuStack::new();
        stack.open(
            "root",
            MenuRowLayout::Stacked,
            vec![MenuRow::action("a", "d", 1)],
        );
        stack.select_next("");
        assert_eq!(
            stack.confirm(&mut String::new()),
            MenuConfirmResult::Apply(1)
        );
    }
}
