use crate::app::{
    AppState,
    command::{CommandActionArgs, SlashAction, action_for_command_catalog},
    effect::Effect,
};
use piko_protocol::CommandCatalogAction;

impl AppState {
    // ── slash command parsing ─────────────────────────────────────────────────

    pub fn try_slash_command(&mut self, text: &str) -> Option<Vec<Effect>> {
        let mut parts = text.split_whitespace();
        let command = parts.next()?;
        let action = self
            .command_catalog
            .iter()
            .find(|item| item.slash_name == command)
            .map(|item| item.action.clone())?;

        let effects = match action {
            CommandCatalogAction::ForkSession => {
                let entry_id = parts
                    .next()
                    .map(ToString::to_string)
                    .or_else(|| self.tree.selected_filtered_entry_id());
                self.dispatch_catalog_action(&action, entry_id, None)
            }
            CommandCatalogAction::RenameSession => {
                let name = parts.collect::<Vec<_>>().join(" ");
                if name.is_empty() {
                    self.status = "usage: /name <session name>".to_string();
                    Vec::new()
                } else {
                    self.dispatch(SlashAction::Rename(name).into())
                }
            }
            CommandCatalogAction::ImportSession => {
                let path = parts.collect::<Vec<_>>().join(" ");
                if path.is_empty() {
                    self.status = "usage: /import <jsonl path>".to_string();
                    Vec::new()
                } else {
                    self.dispatch(SlashAction::Import(path).into())
                }
            }
            CommandCatalogAction::ExportSession => {
                if let Some(ref session_id) = self.session.id {
                    self.status = format!(
                        "Session saved in ~/.piko/agent/sessions/ under ID: {}",
                        session_id
                    );
                } else {
                    self.status = "No active session".to_string();
                }
                Vec::new()
            }
            CommandCatalogAction::DeleteSession => {
                if parts.next() == Some("confirm") {
                    self.dispatch(SlashAction::Delete.into())
                } else {
                    self.status = "usage: /delete confirm".to_string();
                    Vec::new()
                }
            }
            CommandCatalogAction::Login => {
                let provider = parts.next().map(|s| s.to_string());
                self.dispatch_catalog_action(&action, None, provider)
            }
            CommandCatalogAction::Logout => {
                let provider = parts.next().map(|s| s.to_string());
                self.dispatch_catalog_action(&action, None, provider)
            }
            CommandCatalogAction::SetThinking { level } => {
                self.run_command_action(CommandCatalogAction::SetThinking { level })
            }
            CommandCatalogAction::ToggleToolsExpanded => {
                self.run_command_action(CommandCatalogAction::ToggleToolsExpanded)
            }
            _ => self.dispatch_catalog_action(&action, None, None),
        };
        Some(effects)
    }

    fn dispatch_catalog_action(
        &mut self,
        action: &CommandCatalogAction,
        fork_entry_id: Option<String>,
        provider: Option<String>,
    ) -> Vec<Effect> {
        if let Some(action) = action_for_command_catalog(
            action,
            CommandActionArgs {
                fork_entry_id,
                provider,
                clear_notifications_and_close: true,
            },
        ) {
            self.dispatch(action)
        } else {
            Vec::new()
        }
    }
}
