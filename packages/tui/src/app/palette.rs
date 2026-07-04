use piko_protocol::CommandCatalogAction;

use crate::{
    Effect,
    app::{AppState, command::CommandActionArgs, command::action_for_command_catalog},
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
            _ => {}
        }
        effects
    }
}
