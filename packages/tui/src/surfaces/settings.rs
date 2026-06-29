use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear, List},
};

use super::selector_item;

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

/// Settings overlay: static list of runtime-configurable options.
pub struct SettingsOverlay {
    pub options: Vec<SettingsOption>,
    pub selected: usize,
}

impl SettingsOverlay {
    pub fn new() -> Self {
        Self {
            options: default_settings_options(),
            selected: 0,
        }
    }

    pub fn select_next(&mut self) {
        if !self.options.is_empty() {
            self.selected = (self.selected + 1).min(self.options.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_option(&self) -> Option<&SettingsOption> {
        self.options.get(self.selected)
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Clear, area);
        let items = self
            .options
            .iter()
            .enumerate()
            .map(|(idx, option)| {
                selector_item(
                    idx == self.selected,
                    option.title.to_string(),
                    option.detail.to_string(),
                )
            })
            .collect::<Vec<_>>();
        let widget = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("settings | j/k select | Enter apply | Esc close"),
        );
        frame.render_widget(widget, area);
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
