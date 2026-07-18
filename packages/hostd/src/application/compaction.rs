use crate::api::SessionTreeEntry;
use crate::application::host_app::HostApp;
use crate::application::sessions::helpers::session_reconciled_message;
use crate::domain::compaction::{
    CompactionSettings, active_branch_entries, context_entries_after_compaction, should_compact,
};
use crate::util::{ClientEventSender, send_event};

impl HostApp {
    pub(crate) async fn resolved_model_context_window(&self) -> u64 {
        let (model, provider, fallback) = {
            let settings = self.settings.lock().await;
            let reserve = settings
                .compaction
                .as_ref()
                .and_then(|value| value.reserve_tokens)
                .unwrap_or(16384);
            let recent = settings
                .compaction
                .as_ref()
                .and_then(|value| value.keep_recent_tokens)
                .unwrap_or(20000);
            (
                settings.default_model.clone(),
                settings.default_provider.clone(),
                reserve + recent,
            )
        };
        self.model_registry
            .lock()
            .await
            .resolve(model.as_deref(), provider.as_deref())
            .map(|resolved| resolved.model.context_window)
            .filter(|window| *window > 0)
            .unwrap_or(fallback)
    }

    pub(crate) async fn compact_session_if_needed(
        &self,
        _command_id: &str,
        session_id: &str,
        agent_instance_id: &str,
        context_window: u64,
        tx: &ClientEventSender,
    ) {
        let c_settings;
        let enabled;
        {
            let settings = self.settings.lock().await;
            c_settings = CompactionSettings {
                enabled: settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.enabled)
                    .unwrap_or(true),
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
            enabled = c_settings.enabled;
        }
        if !enabled {
            return;
        }

        let state_lock = self.state.lock().await;
        let Ok(session) = state_lock.session(session_id) else {
            return;
        };
        let root_agent_instance_id = format!("agent_{session_id}_root");
        if agent_instance_id != root_agent_instance_id {
            // SessionTreeEntry compaction currently projects the root shard.
            // Never compact a different AgentInstance through root state.
            return;
        }
        let mut branch_entries =
            active_branch_entries(&session.entries, session.current_leaf_id.as_deref());
        // Compaction tips with no parent collapse the branch to a single stub;
        // fall back to the full tree so context_entries_after_compaction can expand.
        if branch_entries.len() <= 1 {
            branch_entries = session.entries.clone();
        }
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
        let mut cut_index = cut_point.first_kept_entry_index;

        // SessionCompact passes context_window = 0 to force a rewrite even when the
        // keep_recent waterline would otherwise retain the entire short branch.
        if cut_index == 0 && context_window == 0 && context_entries.len() > 1 {
            cut_index = context_entries.len() - 1;
        }

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

        let Some(summary) = summary else {
            return;
        };

        let first_kept_id = retained_entries
            .first()
            .map(|entry| entry.id().to_string())
            .unwrap_or_default();
        // Attach under the previous tip so the active branch still reaches messages.
        let parent_id = context_entries.last().map(|entry| entry.id().to_string());

        let mut state = self.state.lock().await;
        let mut compacted = false;
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
                compacted = true;
            }
        }
        drop(state);

        // H2: compact that rewrites the projected tree must rebuild via reconcile.
        if compacted && let Ok((snapshot, agents)) = self.session_view(session_id).await {
            send_event(
                tx,
                session_reconciled_message(
                    session_id.to_string(),
                    piko_protocol::ReconcileReason::ExplicitRefresh,
                    snapshot,
                    agents,
                ),
            )
            .await;
        }
    }
}
