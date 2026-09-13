use async_trait::async_trait;

use crate::api::{Command, ProtocolError, ServerMessage};
use crate::domain::config::HostSettings;

use crate::protocol::{HostServer, now_ms};

fn server_response_ok(command_id: &str, result: crate::api::CommandResult) -> ServerMessage {
    ServerMessage::CommandResponse {
        command_id: command_id.to_string(),
        result: Ok(result),
    }
}

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

/// Rebuilds the LLM/orchestrator runner when a runner-frozen setting changes.
struct ModelRunnerObserver;

#[async_trait]
impl ConfigObserver for ModelRunnerObserver {
    async fn on_change(
        &self,
        server: &HostServer,
        old: &HostSettings,
        new: &HostSettings,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let changed = runner_settings_changed(old, new);

        if changed {
            // settings are published only after observers succeed; rebuild
            // against the candidate settings so the new runner matches the
            // durable commit
            server.rebuild_agent_runner_with(new.clone()).await;
        }

        let model_id = new.default_model.clone().unwrap_or_default();
        let provider = new.default_provider.clone().unwrap_or_default();
        let thinking_level = new.default_thinking_level.clone();
        // Host-authoritative window for client chrome (F-22 / D-34 slice 1).
        // Only resolve when a model id is configured so empty defaults do not
        // advertise a hard-fallback catalog model's window.
        let context_window = if model_id.is_empty() {
            None
        } else {
            server
                .model_registry
                .lock()
                .await
                .resolve(Some(model_id.as_str()), provider_hint(provider.as_str()))
                .map(|resolved| resolved.model.context_window)
                .filter(|window| *window > 0)
        };

        Ok(vec![ServerMessage::Model(
            crate::api::ModelEvent::ConfigChanged {
                model_id,
                provider,
                thinking_level,
                context_window,
                timestamp: now_ms(),
            },
        )])
    }
}

fn runner_settings_changed(old: &HostSettings, new: &HostSettings) -> bool {
    new.default_model != old.default_model
        || new.default_provider != old.default_provider
        || new.default_thinking_level != old.default_thinking_level
        || new.retry != old.retry
        || new.approvals != old.approvals
        || new.guardian != old.guardian
        || new.safety != old.safety
        || new.permissions != old.permissions
        || new.features != old.features
        || new.execution != old.execution
        || new.agent_runtime != old.agent_runtime
        || new.mcp_servers != old.mcp_servers
        || new.mcp != old.mcp
        || new.transcript != old.transcript
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
        let mut events = Vec::new();
        let changed = new.default_model != old.default_model
            || new.default_provider != old.default_provider
            || new.default_thinking_level != old.default_thinking_level;

        if changed {
            let thinking_changed = new.default_thinking_level != old.default_thinking_level;

            let thinking_level = new.default_thinking_level.clone();

            if let Some(storage) = &server.storage {
                let session_paths: Vec<(String, std::path::PathBuf)> = {
                    let paths = server.session_paths.lock().await;
                    paths
                        .iter()
                        .map(|(id, p)| (id.clone(), p.clone()))
                        .collect()
                };
                for (session_id, path) in session_paths {
                    let parent_id = {
                        let state = server.state.lock().await;
                        state
                            .session(&session_id)
                            .ok()
                            .and_then(|s| s.current_leaf_id.clone())
                    };
                    // Model continuity is recorded at turn submission from the
                    // durable session record (single source of truth). Config
                    // writes only persist the thinking-level marker; a model
                    // setting change without an executing turn is a live
                    // `ModelEvent::ConfigChanged`, not a session timeline fact.
                    match storage
                        .append_config_metadata(
                            &path,
                            parent_id.as_deref(),
                            None,
                            None,
                            if thinking_changed {
                                thinking_level.as_ref().map(|t| t.as_str())
                            } else {
                                None
                            },
                            None,
                        )
                        .await
                    {
                        Ok(entries) => {
                            {
                                let mut state = server.state.lock().await;
                                for entry in &entries {
                                    let _ = state.append_entry(&session_id, entry.clone());
                                }
                            }
                            for entry in entries {
                                events.push(ServerMessage::SessionEntryCommitted(
                                    piko_protocol::SessionEntryCommittedEvent {
                                        session_id: session_id.clone(),
                                        entry,
                                    },
                                ));
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to persist config metadata for session {session_id}: {e}"
                            );
                        }
                    }
                }
            }
        }
        Ok(events)
    }
}

/// Observer responsible for persisting updated settings to disk.
///
/// Ordering note (P2-7): `apply_config_update` runs observers *before*
/// publishing the new settings into memory, so a disk failure aborts the
/// update and the in-memory/runner state never diverges from durable state.
/// Writes are atomic (temp file + rename) so a crash cannot leave a
/// truncated settings file behind.
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
            let Some(path) = settings_path else {
                return Ok(Vec::new());
            };
            write_settings_atomically(&path, new).map_err(|error| {
                ProtocolError::InvalidCommand(format!(
                    "failed to persist settings to {}: {error}",
                    path.display()
                ))
            })?;
        }
        Ok(Vec::new())
    }
}

/// Serialize settings to a sibling temp file and rename it over the target,
/// so the settings file is never observed half-written.
fn write_settings_atomically(
    path: &std::path::Path,
    settings: &HostSettings,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(settings)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, content)?;
    match std::fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
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
            Ok(vec![server_response_ok(
                "config_update",
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
        command_id: &str,
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

        // 5. Transaction order (P2-7): durable commit first, then publish.
        //    DiskPersistenceObserver writes atomically and propagates errors,
        //    so a failed write leaves the in-memory settings, the agent
        //    runner, and the client view untouched. Observers run while the
        //    settings lock is still held: the in-memory settings are only
        //    published after every observer (including the runner rebuild)
        //    succeeded.
        let observers: Vec<Box<dyn ConfigObserver>> = vec![
            Box::new(DiskPersistenceObserver),
            Box::new(ModelRunnerObserver),
            Box::new(SessionStorageObserver),
            Box::new(TuiSettingsObserver),
        ];

        let mut events = Vec::new();
        for observer in &observers {
            let mut obs_events = observer
                .on_change(self, &old_settings, &new_settings)
                .await?;
            // Config observers may emit a client-facing response (currently
            // the opaque TUI namespace observer). Preserve the command
            // correlation established at the JSONL boundary.
            for event in &mut obs_events {
                if let ServerMessage::CommandResponse {
                    command_id: response_id,
                    ..
                } = event
                    && response_id == "config_update"
                {
                    *response_id = command_id.to_string();
                }
            }
            events.append(&mut obs_events);
        }

        // 6. Durable commit succeeded: publish the in-memory settings.
        *settings_lock = new_settings.clone();
        drop(settings_lock);

        Ok(events)
    }
}

fn provider_hint(provider: &str) -> Option<&str> {
    if provider.is_empty() {
        None
    } else {
        Some(provider)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_rebuild_predicate_covers_frozen_policy_but_not_host_only_settings() {
        let old = HostSettings::default();
        let mut runtime = old.clone();
        runtime.safety = Some(crate::domain::config::SafetySettings {
            auto_approve_workspace_writes: Some(false),
        });
        assert!(runner_settings_changed(&old, &runtime));

        let mut agent_limits = old.clone();
        agent_limits.agent_runtime = Some(crate::domain::config::AgentRuntimeSettings {
            max_agents: Some(4),
            max_depth: Some(2),
        });
        assert!(runner_settings_changed(&old, &agent_limits));

        let mut host_only = old.clone();
        host_only.observability = Some(crate::domain::config::ObservabilitySettings {
            enabled: Some(true),
            ..Default::default()
        });
        assert!(!runner_settings_changed(&old, &host_only));
    }
}
