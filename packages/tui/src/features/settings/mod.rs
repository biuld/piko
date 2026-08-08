//! Settings panel — surface composition over the Settings component kit.

mod catalog;
mod mirror;

pub use catalog::{SettingsAction, action_requires_hostd_restart, build_thinking_choice};
pub use mirror::{HostRuntimeSettings, SettingsSnapshot};

use ratatui::{Frame, layout::Rect};

use crate::{
    theme::Theme,
    ui::components::setting::{SettingConfirmResult, SettingsNavStack},
};

use catalog::build_catalog;

/// Result of confirming a settings row (compatible name for session_ops).
pub type SettingsConfirmResult = SettingConfirmResult<SettingsAction>;

/// Settings panel: NavStack + filter + catalog built from snapshot.
pub struct SettingsPanel {
    pub stack: SettingsNavStack<SettingsAction>,
    pub filter: String,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            stack: SettingsNavStack::new(),
            filter: String::new(),
        }
    }

    pub fn open_root(&mut self, snap: &SettingsSnapshot) {
        self.filter.clear();
        self.stack.open_catalog("settings", build_catalog(snap));
    }

    pub fn open_thinking(&mut self, snap: &SettingsSnapshot) {
        self.filter.clear();
        self.stack.open_choice(build_thinking_choice(snap));
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
        self.stack.render(frame, area, &self.filter, theme);
    }
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self::new()
    }
}
