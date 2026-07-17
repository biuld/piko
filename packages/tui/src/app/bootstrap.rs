use piko_protocol::{Command, SessionListScope};

use super::{AppState, command_id, effect, pending};

impl AppState {
    pub fn bootstrap(&mut self) -> Vec<effect::Effect> {
        let mut effects = Vec::new();
        let config_id = command_id();
        self.session.pending.track(
            config_id.clone(),
            pending::PendingCommandKind::BootstrapConfig,
        );
        effects.push(effect::Effect::send(Command::ConfigGet {
            command_id: config_id,
            namespace: "tui".to_string(),
        }));
        let catalog_id = command_id();
        self.session.pending.track(
            catalog_id.clone(),
            pending::PendingCommandKind::BootstrapCatalog,
        );
        effects.push(effect::Effect::send(Command::CommandCatalogGet {
            command_id: catalog_id,
        }));

        effects.extend(self.bootstrap_config());
        if let Some(session_id) = self.session.requested_id.clone() {
            self.agent_panel.begin_loading();
            self.session.opening_id = Some(session_id.clone());
            let open_id = command_id();
            self.session
                .pending
                .track(open_id.clone(), pending::PendingCommandKind::SessionOpen);
            effects.push(effect::Effect::send(Command::SessionOpen {
                command_id: open_id,
                session_id,
                session_path: None,
            }));
            self.status = "opening session".to_string();
        } else if self.session.continue_requested {
            let list_id = command_id();
            self.session
                .pending
                .track(list_id.clone(), pending::PendingCommandKind::SessionList);
            effects.push(effect::Effect::send(Command::SessionList {
                command_id: list_id,
                scope: SessionListScope::All,
                cwd: None,
            }));
            self.status = "loading sessions".to_string();
        } else {
            self.status = "ready".to_string();
        }
        effects
    }

    fn bootstrap_config(&mut self) -> Vec<effect::Effect> {
        let mut effects = Vec::new();
        if let (Some(provider), Some(api_key)) = (
            self.initial_options.provider.clone(),
            self.initial_options.api_key.clone(),
        ) {
            effects.push(effect::Effect::send(Command::AuthSetApiKey {
                command_id: command_id(),
                provider,
                api_key,
            }));
        }

        let mut patch = serde_json::Map::new();
        if let Some(ref provider) = self.initial_options.provider {
            patch.insert("default-provider".to_string(), serde_json::json!(provider));
        }
        if let Some(ref model_id) = self.initial_options.model_id {
            patch.insert("default-model".to_string(), serde_json::json!(model_id));
        }
        if let Some(ref thinking_level) = self.initial_options.thinking_level {
            patch.insert(
                "default-thinking-level".to_string(),
                serde_json::json!(thinking_level),
            );
        }
        if self.initial_options.no_tools {
            patch.insert(
                "active-tool-names".to_string(),
                serde_json::json!(Vec::<String>::new()),
            );
        }

        effects.push(effect::Effect::send(Command::ConfigUpdate {
            command_id: command_id(),
            patch: serde_json::Value::Object(patch),
        }));
        effects
    }
}
