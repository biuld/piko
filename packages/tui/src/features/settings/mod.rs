//! Settings panel — surface composition over the shared MenuStack + Pane.

mod catalog;
mod mirror;

pub use catalog::{SettingsAction, action_requires_hostd_restart, thinking_level_detail};
pub use mirror::{HostRuntimeSettings, SettingsSnapshot};

use ratatui::{Frame, layout::Rect};

use crate::{
    theme::Theme,
    ui::components::{
        feedback::{settings_apply_hints, settings_open_hints},
        menu::{MenuConfirmResult, MenuRowLayout, MenuStack},
        pane::{PaneMode, PaneSpec, PaneTitleAffix},
        selectable_list::{SelectableItem, render_selectable_list_with_pane},
    },
};

use catalog::build_catalog;

/// Result of confirming a settings row (compatible name for session_ops).
pub type SettingsConfirmResult = MenuConfirmResult<SettingsAction>;

/// Settings panel: MenuStack + filter + catalog built from snapshot.
pub struct SettingsPanel {
    pub stack: MenuStack<SettingsAction>,
    pub filter: String,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            stack: MenuStack::new(),
            filter: String::new(),
        }
    }

    pub fn open_root(&mut self, snap: &SettingsSnapshot) {
        self.filter.clear();
        self.stack
            .open("settings", MenuRowLayout::SettingsRow, build_catalog(snap));
    }

    pub fn pop(&mut self) -> bool {
        self.filter.clear();
        self.stack.pop()
    }

    pub fn select_next(&mut self) {
        self.stack.select_next(&self.filter);
    }

    pub fn select_prev(&mut self) {
        self.stack.select_prev(&self.filter);
    }

    pub fn reset_selection(&mut self) {
        self.stack.reset_selection();
    }

    pub fn confirm(&mut self) -> SettingsConfirmResult {
        self.stack.confirm(&mut self.filter)
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let Some(current) = self.stack.current() else {
            return;
        };
        let at_root = self.stack.at_root();
        let title = if at_root { "Settings" } else { &current.title };
        let items: Vec<SelectableItem> = current
            .items
            .items
            .iter()
            .map(|row| row.to_item(current.layout))
            .collect();
        let hints = match current.layout {
            MenuRowLayout::SettingsOption => settings_apply_hints(),
            _ => settings_open_hints(at_root),
        };

        let mut spec = PaneSpec::new(title)
            .mode(PaneMode::Standard)
            .search_filter(&self.filter)
            .search_rule(true)
            .hints(hints)
            .focused(true);
        if at_root {
            spec = spec.affix(PaneTitleAffix::Close);
        }
        render_selectable_list_with_pane(
            frame,
            area,
            spec,
            &items,
            current.items.selected,
            &self.filter,
            theme,
        );
    }
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self::new()
    }
}
