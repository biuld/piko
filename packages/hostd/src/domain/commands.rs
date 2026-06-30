use crate::api::{CommandCatalogAction, CommandCatalogItem};

pub fn command_catalog() -> Vec<CommandCatalogItem> {
    use CommandCatalogAction::*;

    vec![
        item(
            "help",
            "Help",
            "Show keyboard shortcuts and slash commands",
            Help,
            &["/help", "/?"],
        ),
        item(
            "sessions",
            "Sessions",
            "List and open hostd sessions",
            Sessions,
            &["/sessions", "/session", "/resume"],
        ),
        item(
            "tree",
            "Session tree",
            "Inspect and navigate the current session branch tree",
            Tree,
            &["/tree", "/branches"],
        ),
        item(
            "models",
            "Models",
            "List and set default model",
            Models,
            &["/models", "/model"],
        ),
        item(
            "settings",
            "Settings",
            "Open hostd-backed runtime settings",
            Settings,
            &["/settings", "/config"],
        ),
        item(
            "status",
            "Status",
            "Show turn, queue, approval, and tool state",
            Status,
            &["/status"],
        ),
        item(
            "new",
            "New session",
            "Create a fresh session in the current working directory",
            NewSession,
            &["/new"],
        ),
        item(
            "fork",
            "Fork session",
            "Fork current session at the selected tree entry",
            ForkSession,
            &["/fork"],
        ),
        item(
            "clone",
            "Clone session",
            "Clone current session at the current leaf",
            CloneSession,
            &["/clone"],
        ),
        item(
            "rename",
            "Rename session",
            "Rename current session",
            RenameSession,
            &["/name", "/rename"],
        ),
        item(
            "import",
            "Import session",
            "Import a session JSONL file",
            ImportSession,
            &["/import"],
        ),
        item(
            "export",
            "Export session",
            "Show current session JSONL file path",
            ExportSession,
            &["/export"],
        ),
        item(
            "delete",
            "Delete session",
            "Delete current session; requires confirm",
            DeleteSession,
            &["/delete"],
        ),
        item(
            "login",
            "Login",
            "Start OAuth login, optional provider argument",
            Login,
            &["/login"],
        ),
        item(
            "logout",
            "Logout",
            "Remove credentials, optional provider argument",
            Logout,
            &["/logout"],
        ),
        item(
            "compact",
            "Compact session",
            "Request hostd session compaction",
            Compact,
            &["/compact"],
        ),
        item(
            "thinking.off",
            "Thinking off",
            "Set default thinking level to off",
            SetThinking {
                level: "off".into(),
            },
            &["/thinking-off"],
        ),
        item(
            "thinking.medium",
            "Thinking medium",
            "Set default thinking level to medium",
            SetThinking {
                level: "medium".into(),
            },
            &["/thinking-medium"],
        ),
        item(
            "thinking.high",
            "Thinking high",
            "Set default thinking level to high",
            SetThinking {
                level: "high".into(),
            },
            &["/thinking-high"],
        ),
        item(
            "tools.toggle",
            "Toggle tool details",
            "Switch between folded and expanded tool result rendering",
            ToggleToolsExpanded,
            &["/tools", "/toggle-tools"],
        ),
        item(
            "notifications.clear",
            "Clear notifications",
            "Dismiss all notification messages",
            ClearNotifications,
            &["/clear-notifications", "/clear"],
        ),
        item("quit", "Quit", "Exit the TUI", Quit, &["/quit", "/exit"]),
    ]
}

fn item(
    id: &str,
    title: &str,
    detail: &str,
    action: CommandCatalogAction,
    slash_names: &[&str],
) -> CommandCatalogItem {
    CommandCatalogItem {
        id: id.to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        action,
        slash_names: slash_names.iter().map(|name| (*name).to_string()).collect(),
        visible_in_palette: true,
    }
}
