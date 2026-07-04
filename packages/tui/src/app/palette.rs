use piko_protocol::{Command, CommandCatalogAction};

use crate::{
    app::{
        AppState,
        command::{CommandActionArgs, action_for_command_catalog},
        command_id,
        effect::Effect,
    },
    features::notifications::NotificationLevel,
};

impl AppState {
    pub fn run_command_action(&mut self, action: CommandCatalogAction) -> Vec<Effect> {
        let mut effects = Vec::new();
        if let Some(action) = action_for_command_catalog(
            &action,
            CommandActionArgs {
                fork_entry_id: self.tree.selected_filtered_entry_id(),
                provider: None,
                clear_notifications_and_close: true,
            },
        ) {
            effects.extend(self.dispatch(action));
            return effects;
        }

        match action {
            CommandCatalogAction::RenameSession
            | CommandCatalogAction::ImportSession
            | CommandCatalogAction::ExportSession
            | CommandCatalogAction::DeleteSession => {
                self.status = "this command requires slash arguments".to_string();
            }
            CommandCatalogAction::SetThinking { level } => {
                effects.push(Effect::send(Command::ConfigUpdate {
                    command_id: command_id(),
                    patch: serde_json::json!({
                        "default-thinking-level": level
                    }),
                }));
                self.clear_focus();
                self.status = format!("thinking level {level}");
            }
            CommandCatalogAction::ToggleToolsExpanded => {
                self.timeline.tools_expanded = !self.timeline.tools_expanded;
                self.clear_focus();
                self.status = if self.timeline.tools_expanded {
                    "tool details expanded".to_string()
                } else {
                    "tool details folded".to_string()
                };
                self.notify(NotificationLevel::Info, self.status.clone());
            }
            _ => unreachable!("handled by action_for_command_catalog"),
        }
        effects
    }
}
