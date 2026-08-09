use piko_protocol::Command;

use crate::{
    app::{AppState, SurfaceId, command_id, config_command_for_setting, effect::Effect},
    features::{notifications::NotificationLevel, settings::action_requires_hostd_restart},
    ui::components::menu::MenuConfirmResult,
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
        self.session
            .pending
            .track(command_id, super::pending::PendingCommandKind::SessionList);
        self.push_surface(SurfaceId::Sessions);
        self.status = "loading sessions".to_string();
        effects
    }

    pub(crate) fn request_models(&mut self) -> Vec<Effect> {
        let command_id = command_id();
        self.session.pending.track(
            command_id.clone(),
            super::pending::PendingCommandKind::ModelList,
        );
        let effects = vec![Effect::send(Command::ModelList { command_id })];
        self.push_surface(SurfaceId::Models);
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
        self.begin_session_hydration(Some(summary.session_id.clone()));
        let command_id = command_id();
        effects.push(Effect::send(Command::SessionOpen {
            command_id: command_id.clone(),
            session_id: summary.session_id,
            session_path: summary.session_path,
        }));
        self.session
            .pending
            .track(command_id, super::pending::PendingCommandKind::SessionOpen);
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
        if let Some(model) = self.models.confirm() {
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
        } else {
            self.status = "no model selected".to_string();
        }
        effects
    }

    pub(crate) fn apply_selected_thinking(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        if let Some(level) = self.thinking.confirm() {
            self.model.active_thinking_level = Some(level.to_string());
            self.host_settings.thinking_level = Some(level.to_string());
            effects.push(Effect::send(config_command_for_setting(
                crate::features::settings::SettingsAction::Thinking(level),
            )));
            self.clear_focus();
            self.status = format!("thinking level {level}");
        } else {
            self.status = "no thinking level selected".to_string();
        }
        effects
    }

    pub(crate) fn apply_selected_setting(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        match self.settings.confirm() {
            MenuConfirmResult::Drilled => {}
            MenuConfirmResult::Apply(action) => {
                self.apply_settings_action_optimistically(&action);
                let restart = action_requires_hostd_restart(&action);
                effects.push(Effect::send(config_command_for_setting(action.clone())));
                self.clear_focus();
                self.status = if restart {
                    "setting saved — restart hostd to apply".to_string()
                } else {
                    "setting applied".to_string()
                };
                self.notify(NotificationLevel::Info, self.status.clone());
            }
            MenuConfirmResult::None => {
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
        self.begin_session_hydration(None);
        let fork_id = command_id();
        self.session.pending.track(
            fork_id.clone(),
            super::pending::PendingCommandKind::SessionOpen,
        );
        effects.push(Effect::send(Command::SessionFork {
            command_id: fork_id,
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
        let command_id = command_id();
        effects.push(Effect::send(Command::SessionDelete {
            command_id: command_id.clone(),
            session_id: session_id.clone(),
        }));
        self.session.pending.track(
            command_id,
            super::pending::PendingCommandKind::SessionDelete,
        );
        self.session.pending.delete_session_id = Some(session_id);
        self.status = "deleting session…".to_string();
        effects
    }
}
