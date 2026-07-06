use piko_protocol::Command;

use crate::{
    app::{AppMode, AppState, command_id, config_command_for_setting, effect::Effect},
    features::notifications::NotificationLevel,
    ui::components::hierarchical_menu::MenuConfirmResult,
};

impl AppState {
    pub(crate) fn request_sessions(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        self.sessions.loading = true;
        let scope = self.sessions.scope.to_protocol();
        let cwd = Some(self.cwd.to_string_lossy().into_owned());
        let command_id = command_id();
        effects.push(Effect::send(Command::SessionList {
            command_id: command_id.clone(),
            scope,
            cwd,
        }));
        self.session.pending_list_command_id = Some(command_id);
        self.push_focus(AppMode::Sessions);
        self.status = "loading sessions".to_string();
        effects
    }

    pub(crate) fn request_models(&mut self) -> Vec<Effect> {
        let effects = vec![Effect::send(Command::ModelList {
            command_id: command_id(),
        })];
        self.push_focus(AppMode::Models);
        self.status = "loading models".to_string();
        effects
    }

    pub(crate) fn open_selected_session(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(summary) = self.sessions.selected_session_summary() else {
            self.status = "no session selected".to_string();
            return effects;
        };
        self.sessions.loading = true;
        let command_id = command_id();
        effects.push(Effect::send(Command::SessionOpen {
            command_id: command_id.clone(),
            session_id: summary.session_id,
            session_path: summary.session_path,
        }));
        self.session.pending_open_command_id = Some(command_id);
        self.status = "opening session".to_string();
        effects
    }

    pub(crate) fn navigate_selected_tree_entry(
        &mut self,
        entry_id: String,
        summarize: bool,
        custom_instructions: Option<String>,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(session_id) = self.session.id.clone() else {
            self.status = "no active session".to_string();
            return effects;
        };
        effects.push(Effect::send(piko_protocol::Command::SessionNavigate {
            command_id: command_id(),
            session_id,
            entry_id,
            summarize,
            custom_instructions,
        }));
        self.clear_focus();
        self.status = "navigating session tree".to_string();
        effects
    }

    pub(crate) fn apply_selected_model(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        match self.models.confirm() {
            MenuConfirmResult::Action(model, _) => {
                let provider = model.provider.clone();
                let model_id = model.id.clone();
                effects.push(Effect::send(Command::ConfigUpdate {
                    command_id: command_id(),
                    patch: serde_json::json!({
                        "default-provider": provider,
                        "default-model": model_id,
                    }),
                }));
                self.clear_focus();
                self.status = format!("switching model to {provider}/{model_id}");
            }
            _ => {
                self.status = "no model selected".to_string();
            }
        }
        effects
    }

    pub(crate) fn apply_selected_setting(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        use crate::features::settings::SettingsConfirmResult;
        match self.settings.confirm() {
            SettingsConfirmResult::SubMenuPushed => {}
            SettingsConfirmResult::Action(action, title) => {
                let command = config_command_for_setting(action);
                effects.push(Effect::send(command));
                self.clear_focus();
                self.status = format!("setting applied: {}", title);
                self.notify(NotificationLevel::Info, self.status.clone());
            }
            SettingsConfirmResult::None => {
                self.status = "no setting selected".to_string();
            }
        }
        effects
    }

    pub(crate) fn fork_session(&mut self, entry_id: Option<String>) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(session_id) = self.session.id.clone() else {
            self.status = "no active session to fork".to_string();
            return effects;
        };
        effects.push(Effect::send(Command::SessionFork {
            command_id: command_id(),
            session_id,
            entry_id,
        }));
        self.clear_focus();
        self.status = "forking session".to_string();
        effects
    }

    pub(crate) fn rename_session(&mut self, name: String) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(session_id) = self.session.id.clone() else {
            self.status = "no active session to rename".to_string();
            return effects;
        };
        effects.push(Effect::send(Command::SessionRename {
            command_id: command_id(),
            session_id,
            name: name.clone(),
        }));
        self.status = format!("renaming session to {name}");
        self.notify(NotificationLevel::Info, self.status.clone());
        effects
    }

    pub(crate) fn delete_current_session(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(session_id) = self.session.id.clone() else {
            self.status = "no active session to delete".to_string();
            return effects;
        };
        effects.push(Effect::send(Command::SessionDelete {
            command_id: command_id(),
            session_id,
        }));
        self.session.id = None;
        self.timeline.clear();
        self.tree.document = Default::default();
        self.tree.visible.rows.clear();
        self.clear_focus();
        self.status = "session deleted".to_string();
        self.notify(NotificationLevel::Warning, "session deleted");
        effects
    }
}
