use ratatui::{Frame, layout::Rect};

use crate::{
    theme::Theme,
    ui::components::filterable_list::{FilterableItem, FilterableList, render_filterable_list},
};

/// Action applied when a settings option is confirmed.
#[derive(Clone)]
pub enum SettingsAction {
    Thinking(&'static str),
    HideThinking(bool),
    Compaction(bool),
    Theme(&'static str),
    Transport(&'static str),
    DisableTools,
}

/// A single settings option row.
#[derive(Clone)]
pub struct SettingsOption {
    pub title: &'static str,
    pub detail: &'static str,
    pub action: SettingsAction,
}

/// Settings panel: list of runtime-configurable options.
pub struct SettingsPanel {
    pub list: FilterableList<SettingsOption>,
}

impl SettingsPanel {
    pub fn new() -> Self {
        Self {
            list: FilterableList::new(default_settings_options()),
        }
    }

    pub fn select_next(&mut self, filter: &str) {
        self.list.select_next(filter, |item| {
            item.title.to_lowercase().contains(filter)
                || item.detail.to_lowercase().contains(filter)
        });
    }

    pub fn select_prev(&mut self, filter: &str) {
        self.list.select_prev(filter, |item| {
            item.title.to_lowercase().contains(filter)
                || item.detail.to_lowercase().contains(filter)
        });
    }

    pub fn selected_option(&self, filter: &str) -> Option<&SettingsOption> {
        let filtered = self.list.filtered_indices(filter, |item| {
            item.title.to_lowercase().contains(filter)
                || item.detail.to_lowercase().contains(filter)
        });
        if filtered.is_empty() {
            return None;
        }
        let selected_filtered_idx = filtered
            .iter()
            .position(|&orig_idx| orig_idx == self.list.selected)
            .unwrap_or(0)
            .min(filtered.len().saturating_sub(1));
        filtered
            .get(selected_filtered_idx)
            .and_then(|&orig_idx| self.list.items.get(orig_idx))
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, filter: &str, theme: &Theme) {
        let items: Vec<FilterableItem> = self
            .list
            .items
            .iter()
            .map(|item| FilterableItem {
                primary: item.title.to_string(),
                detail: item.detail.to_string(),
                is_active: false,
            })
            .collect();
        render_filterable_list(
            frame,
            area,
            "settings",
            &items,
            self.list.selected,
            filter,
            theme,
        );
    }
}

fn default_settings_options() -> Vec<SettingsOption> {
    vec![
        SettingsOption {
            title: "Thinking off",
            detail: "Set default thinking level to off",
            action: SettingsAction::Thinking("off"),
        },
        SettingsOption {
            title: "Thinking medium",
            detail: "Set default thinking level to medium",
            action: SettingsAction::Thinking("medium"),
        },
        SettingsOption {
            title: "Thinking high",
            detail: "Set default thinking level to high",
            action: SettingsAction::Thinking("high"),
        },
        SettingsOption {
            title: "Hide thinking blocks",
            detail: "Hide thinking content in future rendering where supported",
            action: SettingsAction::HideThinking(true),
        },
        SettingsOption {
            title: "Show thinking blocks",
            detail: "Show thinking content in future rendering where supported",
            action: SettingsAction::HideThinking(false),
        },
        SettingsOption {
            title: "Enable compaction",
            detail: "Enable hostd automatic compaction",
            action: SettingsAction::Compaction(true),
        },
        SettingsOption {
            title: "Disable compaction",
            detail: "Disable hostd automatic compaction",
            action: SettingsAction::Compaction(false),
        },
        SettingsOption {
            title: "Theme dark",
            detail: "Set configured theme to dark",
            action: SettingsAction::Theme("dark"),
        },
        SettingsOption {
            title: "Theme light",
            detail: "Set configured theme to light",
            action: SettingsAction::Theme("light"),
        },
        SettingsOption {
            title: "Transport stdio",
            detail: "Set host transport preference to stdio",
            action: SettingsAction::Transport("stdio"),
        },
        SettingsOption {
            title: "Disable tools",
            detail: "Set active tools to an empty list",
            action: SettingsAction::DisableTools,
        },
    ]
}
