use async_trait::async_trait;
use std::sync::Arc;

use crate::api::{Command, ProtocolError, ServerMessage};
use crate::domain::config::HostSettings;
use crate::domain::turns::{ErrorTurnRunner, TurnRunner};

use crate::protocol::{HostServer, build_orch_turn_runner, now_ms};

/// Abstract Configuration Observer.
/// Custom business logic triggered upon configuration changes implements this trait.
#[async_trait]
trait ConfigObserver: Send + Sync {
    async fn on_change(
        &self,
        server: &HostServer,
        old: &HostSettings,
        new: &HostSettings,
    ) -> Result<Vec<ServerMessage>, ProtocolError>;
}

/// Observer responsible for rebuilding the LLM orchestration turn runner when model parameters change.
struct ModelRunnerObserver;

#[async_trait]
impl ConfigObserver for ModelRunnerObserver {
    async fn on_change(
        &self,
        server: &HostServer,
        old: &HostSettings,
        new: &HostSettings,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let changed = new.default_model != old.default_model
            || new.default_provider != old.default_provider
            || new.default_thinking_level != old.default_thinking_level;

        if !changed {
            return Ok(Vec::new());
        }

        let model_id = new.default_model.clone().unwrap_or_default();
        let provider = new.default_provider.clone().unwrap_or_default();
        let thinking_level = new.default_thinking_level.clone();

        let (runner, executor) = build_orch_turn_runner(new).await.unwrap_or_else(|e| {
            (
                Arc::new(ErrorTurnRunner::new(e)) as Arc<dyn TurnRunner>,
                None,
            )
        });
        *server.turn_runner.lock().await = runner;
        if let Some(exec) = executor {
            server.set_model_executor(exec).await;
        }

        Ok(vec![ServerMessage::Model(
            crate::api::ModelEvent::ConfigChanged {
                model_id,
                provider,
                thinking_level,
                timestamp: now_ms(),
            },
        )])
    }
}

/// Observer responsible for logging configuration metadata changes inside active session JSONL files.
struct SessionStorageObserver;

#[async_trait]
impl ConfigObserver for SessionStorageObserver {
    async fn on_change(
        &self,
        server: &HostServer,
        old: &HostSettings,
        new: &HostSettings,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let changed = new.default_model != old.default_model
            || new.default_provider != old.default_provider
            || new.default_thinking_level != old.default_thinking_level;

        if changed {
            let model_id = new.default_model.clone().unwrap_or_default();
            let provider = new.default_provider.clone().unwrap_or_default();
            let thinking_level = new.default_thinking_level.clone();

            if let Some(storage) = &server.storage {
                let paths = server.session_paths.lock().await;
                for (session_id, path) in paths.iter() {
                    let parent_id = {
                        let state = server.state.lock().await;
                        state
                            .session(session_id)
                            .ok()
                            .and_then(|s| s.current_leaf_id.clone())
                    };
                    if let Err(e) = storage.append_config_metadata(
                        path,
                        parent_id.as_deref(),
                        if model_id.is_empty() {
                            None
                        } else {
                            Some(&model_id)
                        },
                        if provider.is_empty() {
                            None
                        } else {
                            Some(&provider)
                        },
                        thinking_level.as_ref().map(|l| l.as_str()),
                        None,
                    ) {
                        tracing::warn!(
                            "Failed to persist config metadata for session {session_id}: {e}"
                        );
                    }
                }
            }
        }
        Ok(Vec::new())
    }
}

/// Observer responsible for persisting updated settings to disk.
struct DiskPersistenceObserver;

#[async_trait]
impl ConfigObserver for DiskPersistenceObserver {
    async fn on_change(
        &self,
        server: &HostServer,
        old: &HostSettings,
        new: &HostSettings,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        if new != old {
            let settings_path = server.project_settings_path.lock().await.clone();
            if let Some(ref path) = settings_path {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(content) = toml::to_string_pretty(new) {
                    let _ = std::fs::write(path, content);
                }
            }
        }
        Ok(Vec::new())
    }
}

/// Observer responsible for updating TUI configuration namespaced settings on client side.
struct TuiSettingsObserver;

#[async_trait]
impl ConfigObserver for TuiSettingsObserver {
    async fn on_change(
        &self,
        _server: &HostServer,
        old: &HostSettings,
        new: &HostSettings,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        if new.tui != old.tui {
            let value = new
                .tui
                .clone()
                .unwrap_or(serde_json::Value::Object(Default::default()));
            Ok(vec![ServerMessage::CommandResult(
                crate::api::CommandResult::ConfigEntry {
                    namespace: "tui".to_string(),
                    value,
                },
            )])
        } else {
            Ok(Vec::new())
        }
    }
}

impl HostServer {
    pub(crate) async fn apply_config_update(
        &self,
        command: Command,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let Command::ConfigUpdate { patch, .. } = command else {
            unreachable!("apply_config_update requires ConfigUpdate")
        };

        // 1. Lock and retrieve current configuration
        let mut settings_lock = self.settings.lock().await;
        let old_settings = settings_lock.clone();

        // 2. Serialize settings to JSON
        let mut settings_json = serde_json::to_value(&old_settings)
            .map_err(|e| ProtocolError::InvalidCommand(e.to_string()))?;

        // 3. Apply JSON Merge Patch (RFC 7386)
        merge_json(&mut settings_json, &patch);

        // 4. Validate structures via deserialization
        let new_settings: HostSettings = serde_json::from_value(settings_json)
            .map_err(|e| ProtocolError::InvalidCommand(format!("Invalid config patch: {}", e)))?;

        // 5. Update state in memory
        *settings_lock = new_settings.clone();
        drop(settings_lock); // Release lock before running observers that may require lock access or filesystem wait

        // 6. Execute registered configuration observers (Hooks)
        let observers: Vec<Box<dyn ConfigObserver>> = vec![
            Box::new(ModelRunnerObserver),
            Box::new(SessionStorageObserver),
            Box::new(DiskPersistenceObserver),
            Box::new(TuiSettingsObserver),
        ];

        let mut events = Vec::new();
        for observer in observers {
            let mut obs_events = observer
                .on_change(self, &old_settings, &new_settings)
                .await?;
            events.append(&mut obs_events);
        }

        Ok(events)
    }
}

/// Dynamic JSON Merge Patch implementation (RFC 7386)
fn merge_json(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (k, v) in patch_map {
                if v.is_null() {
                    base_map.remove(k);
                } else {
                    merge_json(
                        base_map.entry(k.clone()).or_insert(serde_json::Value::Null),
                        v,
                    );
                }
            }
        }
        (base, patch) => {
            *base = patch.clone();
        }
    }
}
