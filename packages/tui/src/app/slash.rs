use crate::{
    app::{AppState, command::Action},
    host::HostdClient,
};
use piko_protocol::CommandCatalogAction;

impl AppState {
    // ── slash command parsing ─────────────────────────────────────────────────

    pub fn try_slash_command(&mut self, host: &mut HostdClient, text: &str) -> bool {
        let mut parts = text.split_whitespace();
        let Some(command) = parts.next() else {
            return false;
        };
        let Some(action) = self
            .command_catalog
            .iter()
            .find(|item| item.slash_names.iter().any(|name| name == command))
            .map(|item| item.action.clone())
        else {
            return false;
        };

        match action {
            CommandCatalogAction::Help => {
                self.dispatch(host, Action::OpenHelp);
                true
            }
            CommandCatalogAction::Commands => {
                self.dispatch(host, Action::OpenCommands);
                true
            }
            CommandCatalogAction::Sessions => {
                self.dispatch(host, Action::RequestSessions);
                true
            }
            CommandCatalogAction::Tree => {
                self.dispatch(host, Action::OpenTree);
                true
            }
            CommandCatalogAction::Models => {
                self.dispatch(host, Action::RequestModels);
                true
            }
            CommandCatalogAction::Settings => {
                self.dispatch(host, Action::OpenSettings);
                true
            }
            CommandCatalogAction::Status => {
                self.dispatch(host, Action::OpenStatus);
                true
            }
            CommandCatalogAction::NewSession => {
                self.dispatch(host, Action::SlashNew);
                true
            }
            CommandCatalogAction::ForkSession => {
                let entry_id = parts
                    .next()
                    .map(ToString::to_string)
                    .or_else(|| self.tree.selected_entry_id(&self.filter_text));
                self.dispatch(host, Action::SlashFork(entry_id));
                true
            }
            CommandCatalogAction::CloneSession => {
                self.dispatch(host, Action::SlashClone);
                true
            }
            CommandCatalogAction::RenameSession => {
                let name = parts.collect::<Vec<_>>().join(" ");
                if name.is_empty() {
                    self.status = "usage: /name <session name>".to_string();
                } else {
                    self.dispatch(host, Action::SlashRename(name));
                }
                true
            }
            CommandCatalogAction::ImportSession => {
                let path = parts.collect::<Vec<_>>().join(" ");
                if path.is_empty() {
                    self.status = "usage: /import <jsonl path>".to_string();
                } else {
                    self.dispatch(host, Action::SlashImport(path));
                }
                true
            }
            CommandCatalogAction::ExportSession => {
                if let Some(ref session_id) = self.session_id {
                    self.status = format!(
                        "Session saved in ~/.piko/agent/sessions/ under ID: {}",
                        session_id
                    );
                } else {
                    self.status = "No active session".to_string();
                }
                true
            }
            CommandCatalogAction::DeleteSession => {
                if parts.next() == Some("confirm") {
                    self.dispatch(host, Action::SlashDelete);
                } else {
                    self.status = "usage: /delete confirm".to_string();
                }
                true
            }
            CommandCatalogAction::Login => {
                let provider = parts.next().unwrap_or("anthropic").to_string();
                self.dispatch(host, Action::SlashLogin(provider));
                true
            }
            CommandCatalogAction::Logout => {
                let provider = parts.next().unwrap_or("anthropic").to_string();
                self.dispatch(host, Action::SlashLogout(provider));
                true
            }
            CommandCatalogAction::Compact => {
                self.dispatch(host, Action::SlashCompact);
                true
            }
            CommandCatalogAction::SetThinking { level } => {
                self.status = format!("usage: command palette only: thinking {level}");
                true
            }
            CommandCatalogAction::ToggleToolsExpanded
            | CommandCatalogAction::ClearNotifications
            | CommandCatalogAction::Quit => {
                self.status = "command palette only".to_string();
                true
            }
        }
    }
}
