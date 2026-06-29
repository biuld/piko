use ratatui::{
    Frame,
    layout::Rect,
};

use super::{SelectListView, SelectItem, SelectorList};

/// A single item in the command palette.
#[derive(Clone)]
pub struct CommandItem {
    pub title: &'static str,
    pub detail: &'static str,
    pub action: CommandAction,
}

/// Actions that the command palette can trigger.
#[derive(Clone)]
pub enum CommandAction {
    Help,
    Sessions,
    Models,
    SessionTree,
    Settings,
    Status,
    NewSession,
    ForkSession,
    CloneSession,
    Login(&'static str),
    Logout(&'static str),
    Compact,
    Thinking(&'static str),
    ToggleToolsExpanded,
    ClearNotifications,
    Quit,
}

/// Command palette overlay: static list of commands with selection state.
pub struct CommandsOverlay {
    pub list: SelectorList<CommandItem>,
}

impl CommandsOverlay {
    pub fn new() -> Self {
        Self {
            list: SelectorList::new(default_commands()),
        }
    }

    pub fn select_next(&mut self, filter: &str) {
        self.list.select_next(filter, |item| {
            item.title.to_lowercase().contains(filter) || item.detail.to_lowercase().contains(filter)
        });
    }

    pub fn select_prev(&mut self, filter: &str) {
        self.list.select_prev(filter, |item| {
            item.title.to_lowercase().contains(filter) || item.detail.to_lowercase().contains(filter)
        });
    }

    pub fn selected_action(&self, filter: &str) -> Option<CommandAction> {
        let filtered = self.list.filtered_indices(filter, |item| {
            item.title.to_lowercase().contains(filter) || item.detail.to_lowercase().contains(filter)
        });
        if filtered.is_empty() {
            return None;
        }
        let selected_filtered_idx = filtered
            .iter()
            .position(|&orig_idx| orig_idx == self.list.selected)
            .unwrap_or(0)
            .min(filtered.len().saturating_sub(1));
        filtered.get(selected_filtered_idx).and_then(|&orig_idx| self.list.items.get(orig_idx)).map(|item| item.action.clone())
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, filter: &str) {
        let select_items: Vec<SelectItem> = self.list.items
            .iter()
            .map(|item| SelectItem {
                primary: item.title.to_string(),
                detail: item.detail.to_string(),
                is_active: false,
            })
            .collect();
        SelectListView::render(frame, area, "commands", &select_items, self.list.selected, filter);
    }
}

fn default_commands() -> Vec<CommandItem> {
    vec![
        CommandItem {
            title: "Help",
            detail: "Show keyboard shortcuts and slash commands",
            action: CommandAction::Help,
        },
        CommandItem {
            title: "Sessions",
            detail: "List and open hostd sessions",
            action: CommandAction::Sessions,
        },
        CommandItem {
            title: "Models",
            detail: "List and set default model",
            action: CommandAction::Models,
        },
        CommandItem {
            title: "Session tree",
            detail: "Inspect and navigate the current session branch tree",
            action: CommandAction::SessionTree,
        },
        CommandItem {
            title: "Status",
            detail: "Show turn, queue, approval, and tool state",
            action: CommandAction::Status,
        },
        CommandItem {
            title: "Settings",
            detail: "Open hostd-backed runtime settings",
            action: CommandAction::Settings,
        },
        CommandItem {
            title: "New session",
            detail: "Create a fresh session in the current working directory",
            action: CommandAction::NewSession,
        },
        CommandItem {
            title: "Fork session",
            detail: "Fork current session at the selected tree entry",
            action: CommandAction::ForkSession,
        },
        CommandItem {
            title: "Clone session",
            detail: "Clone current session at the current leaf",
            action: CommandAction::CloneSession,
        },
        CommandItem {
            title: "Login Anthropic",
            detail: "Start hostd OAuth login for Anthropic",
            action: CommandAction::Login("anthropic"),
        },
        CommandItem {
            title: "Login OpenAI",
            detail: "Start hostd OAuth login for OpenAI",
            action: CommandAction::Login("openai"),
        },
        CommandItem {
            title: "Logout Anthropic",
            detail: "Remove Anthropic credentials from hostd",
            action: CommandAction::Logout("anthropic"),
        },
        CommandItem {
            title: "Compact session",
            detail: "Request hostd session compaction",
            action: CommandAction::Compact,
        },
        CommandItem {
            title: "Thinking off",
            detail: "Set default thinking level to off",
            action: CommandAction::Thinking("off"),
        },
        CommandItem {
            title: "Thinking medium",
            detail: "Set default thinking level to medium",
            action: CommandAction::Thinking("medium"),
        },
        CommandItem {
            title: "Thinking high",
            detail: "Set default thinking level to high",
            action: CommandAction::Thinking("high"),
        },
        CommandItem {
            title: "Toggle tool details",
            detail: "Switch between folded and expanded tool result rendering",
            action: CommandAction::ToggleToolsExpanded,
        },
        CommandItem {
            title: "Clear notifications",
            detail: "Dismiss all notification messages",
            action: CommandAction::ClearNotifications,
        },
        CommandItem {
            title: "Quit",
            detail: "Exit the TUI",
            action: CommandAction::Quit,
        },
    ]
}
