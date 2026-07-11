use tokio::sync::mpsc::UnboundedSender;

use crate::api::{ServerMessage, SessionTreeEntry};
use crate::domain::compaction::{
    CompactionSettings, active_branch_entries, context_entries_after_compaction, should_compact,
};

use crate::protocol::HostServer;

impl HostServer {
    pub(crate) async fn compact_session_if_needed(
        &self,
        _command_id: &str,
        session_id: &str,
        context_window: u64,
        _tx: &UnboundedSender<ServerMessage>,
    ) {
        let c_settings;
        let enabled;
        {
            let settings = self.settings.lock().await;
            let (e, _, _) = (
                settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.enabled)
                    .unwrap_or(true),
                settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.reserve_tokens)
                    .unwrap_or(16384),
                settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.keep_recent_tokens)
                    .unwrap_or(20000),
            );
            c_settings = CompactionSettings {
                enabled: e,
                reserve_tokens: settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.reserve_tokens)
                    .unwrap_or(16384),
                keep_recent_tokens: settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.keep_recent_tokens)
                    .unwrap_or(20000),
            };
            enabled = e;
        }
        if !enabled {
            return;
        }

        let state_lock = self.state.lock().await;
        let branch_entries = state_lock
            .session(session_id)
            .map(|session| {
                active_branch_entries(&session.entries, session.current_leaf_id.as_deref())
            })
            .unwrap_or_default();
        drop(state_lock);

        let context_entries = context_entries_after_compaction(&branch_entries);

        if !should_compact(&context_entries, context_window, &c_settings) {
            return;
        }

        let cut_point = crate::domain::compaction::find_cut_point(
            &context_entries,
            0,
            context_entries.len(),
            c_settings.keep_recent_tokens,
        );
        let cut_index = cut_point.first_kept_entry_index;

        if cut_index == 0 {
            return;
        }

        let entries_to_summarize = &context_entries[0..cut_index];
        let retained_entries = &context_entries[cut_index..];

        let previous_summary = entries_to_summarize
            .iter()
            .rev()
            .find_map(|entry| match entry {
                SessionTreeEntry::Compaction(compaction) => Some(compaction.summary.as_str()),
                _ => None,
            });

        let summary = {
            let executor_guard = self.model_executor.lock().await;
            if let Some(ref executor) = *executor_guard {
                let (model_id, provider) = {
                    let settings = self.settings.lock().await;
                    (
                        settings
                            .default_model
                            .clone()
                            .unwrap_or_else(|| "default".into()),
                        settings
                            .default_provider
                            .clone()
                            .unwrap_or_else(|| "default".into()),
                    )
                };

                let model = piko_protocol::messages::Model {
                    id: model_id.clone(),
                    name: model_id,
                    provider,
                    base_url: None,
                };

                crate::domain::compaction::summarizer::summarize_history(
                    executor.clone(),
                    model,
                    entries_to_summarize,
                    previous_summary,
                    "",
                )
                .await
                .ok()
            } else {
                None
            }
        };

        if let Some(summary) = summary {
            let first_kept_id = retained_entries
                .first()
                .map(|entry| entry.id().to_string())
                .unwrap_or_default();

            let mut state = self.state.lock().await;
            let parent_id = state
                .session(session_id)
                .ok()
                .and_then(|session| session.current_leaf_id.clone());

            if let Some(storage) = &self.storage {
                let path = {
                    let paths = self.session_paths.lock().await;
                    paths.get(session_id).cloned()
                };
                if let Some(path) = path
                    && let Ok(entry) = storage.append_compaction(
                        &path,
                        parent_id.as_deref(),
                        &summary,
                        &first_kept_id,
                        None,
                    )
                {
                    let _ = state.append_entry(session_id, entry);
                }
            }

            drop(state);
        }
    }
}
