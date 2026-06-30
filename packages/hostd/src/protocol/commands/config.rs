use std::sync::Arc;

use crate::api::{Command, Event, ProtocolError};
use crate::domain::config::CompactionSettings;
use crate::domain::turns::{ErrorTurnRunner, TurnRunner};

use crate::protocol::{HostServer, build_orch_turn_runner, now_ms};

impl HostServer {
    pub(crate) async fn apply_config_set(
        &self,
        command: Command,
    ) -> Result<Vec<Event>, ProtocolError> {
        let Command::ConfigSet {
            default_provider,
            default_model,
            default_thinking_level,
            active_tools,
            theme,
            hide_thinking_block,
            transport,
            compaction_enabled,
            compaction_reserve_tokens,
            compaction_keep_recent_tokens,
            ..
        } = command
        else {
            unreachable!("apply_config_set requires ConfigSet")
        };

        let mut settings = self.settings.lock().await;
        if default_provider.is_some() {
            settings.default_provider = default_provider;
        }
        if default_model.is_some() {
            settings.default_model = default_model;
        }
        if let Some(ref level_str) = default_thinking_level
            && let Ok(level) = serde_json::from_str::<piko_protocol::model::ThinkingLevel>(
                &format!("\"{}\"", level_str),
            )
        {
            settings.default_thinking_level = Some(level);
        }
        if active_tools.is_some() {
            settings.active_tool_names = active_tools;
        }
        if theme.is_some() {
            settings.theme = theme;
        }
        if hide_thinking_block.is_some() {
            settings.hide_thinking_block = hide_thinking_block;
        }
        if transport.is_some() {
            settings.transport = transport;
        }

        let comp = settings.compaction.get_or_insert(CompactionSettings {
            enabled: Some(true),
            reserve_tokens: Some(16384),
            keep_recent_tokens: Some(20000),
        });
        if compaction_enabled.is_some() {
            comp.enabled = compaction_enabled;
        }
        if compaction_reserve_tokens.is_some() {
            comp.reserve_tokens = compaction_reserve_tokens;
        }
        if compaction_keep_recent_tokens.is_some() {
            comp.keep_recent_tokens = compaction_keep_recent_tokens;
        }

        let model_id = settings.default_model.clone().unwrap_or_default();
        let provider = settings.default_provider.clone().unwrap_or_default();
        let thinking_level = settings.default_thinking_level.clone();
        let (runner, executor) = build_orch_turn_runner(&settings).await.unwrap_or_else(|e| {
            (
                Arc::new(ErrorTurnRunner::new(e)) as Arc<dyn TurnRunner>,
                None,
            )
        });
        *self.turn_runner.lock().await = runner;
        if let Some(exec) = executor {
            self.set_model_executor(exec).await;
        }

        if let Some(storage) = &self.storage {
            let paths = self.session_paths.lock().await;
            for (session_id, path) in paths.iter() {
                let parent_id = {
                    let state = self.state.lock().await;
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

        let settings_path = self.project_settings_path.lock().await.clone();
        drop(settings);
        if let Some(ref path) = settings_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let merged = self.settings.lock().await.clone();
            if let Ok(content) = toml::to_string_pretty(&merged) {
                let _ = std::fs::write(path, content);
            }
        }

        let ts = now_ms();
        Ok(vec![Event::ModelConfigChanged {
            model_id,
            provider,
            thinking_level,
            timestamp: ts,
        }])
    }
}
