use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear, List},
};

use super::selector_item;

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
    pub items: Vec<CommandItem>,
    pub selected: usize,
}

impl CommandsOverlay {
    pub fn new() -> Self {
        Self {
            items: default_commands(),
            selected: 0,
        }
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1).min(self.items.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_action(&self) -> Option<CommandAction> {
        self.items
            .get(self.selected)
            .map(|item| item.action.clone())
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Clear, area);
        let items = self
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                selector_item(
                    idx == self.selected,
                    item.title.to_string(),
                    item.detail.to_string(),
                )
            })
            .collect::<Vec<_>>();
        let widget = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("commands | j/k select | Enter run | Esc close"),
        );
        frame.render_widget(widget, area);
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
