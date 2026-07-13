use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::util::{now_ms, storage_error};

use super::helpers::server_response_ok;

impl HostApp {
    pub(crate) async fn apply_session_navigate(
        &self,
        command_id: &str,
        session_id: String,
        entry_id: String,
        summarize: bool,
        custom_instructions: Option<String>,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let mut state = self.state.lock().await;
        let session = state.session(&session_id)?;
        if session.active_turn_id.is_some() {
            return Err(ProtocolError::ActiveTurnExists(session_id.clone()));
        }

        let old_leaf_id = session.current_leaf_id.clone();
        let entries = session.entries.clone();

        let mut target_id = Some(entry_id.clone());
        let mut editor_text = None;

        let target_entry = entries
            .iter()
            .find(|e| e.id() == entry_id)
            .cloned()
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(format!("unknown tree entry: {entry_id}"))
            })?;
        match &target_entry {
            crate::api::SessionTreeEntry::Message(m) if m.message.role() == "user" => {
                target_id = m.parent_id.clone();
                editor_text = Some(crate::domain::compaction::entry_text(&target_entry));
            }
            crate::api::SessionTreeEntry::CustomMessage(m) => {
                target_id = m.parent_id.clone();
                editor_text = Some(crate::domain::compaction::entry_text(&target_entry));
            }
            _ => {}
        }

        let mut branch_summary = None;
        if summarize && let Some(old) = old_leaf_id.as_deref() {
            let active_entries =
                crate::domain::compaction::active_branch_entries(&entries, Some(old));

            let mut target_ancestors = std::collections::HashSet::new();
            let mut curr = target_id.clone();
            while let Some(id) = curr {
                target_ancestors.insert(id.clone());
                if let Some(e) = entries.iter().find(|x| x.id() == id) {
                    curr = e.parent_id().map(str::to_string);
                } else {
                    break;
                }
            }

            let common_ancestor = active_entries
                .iter()
                .rev()
                .find(|e| target_ancestors.contains(e.id()))
                .map(|e| e.id().to_string());

            let mut abandoned = Vec::new();
            for e in active_entries {
                if Some(e.id()) == common_ancestor.as_deref() {
                    abandoned.clear();
                    continue;
                }
                abandoned.push(e.clone());
            }

            if !abandoned.is_empty() {
                drop(state);

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

                    let previous_summary = abandoned.iter().rev().find_map(|e| {
                        if let crate::api::SessionTreeEntry::Compaction(c) = e {
                            Some(c.summary.clone())
                        } else {
                            None
                        }
                    });

                    let file_ops_str = custom_instructions
                        .map(|i| format!("\n\nCustom Instructions:\n{}", i))
                        .unwrap_or_default();

                    let summary_text = crate::domain::compaction::summarizer::summarize_history(
                        executor.clone(),
                        model,
                        &abandoned,
                        previous_summary.as_deref(),
                        &file_ops_str,
                    )
                    .await
                    .ok();

                    if let Some(text) = summary_text {
                        let b_entry = crate::api::SessionTreeEntry::BranchSummary(
                            crate::api::BranchSummaryEntry {
                                id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
                                parent_id: target_id.clone(),
                                timestamp: now_ms().to_string(),
                                from_id: old.to_string(),
                                summary: text,
                                details: None,
                                from_hook: None,
                            },
                        );
                        branch_summary = Some(b_entry);
                    }
                }
                state = self.state.lock().await;
                let session = state.session(&session_id)?;
                if session.active_turn_id.is_some() {
                    return Err(ProtocolError::ActiveTurnExists(session_id.clone()));
                }
                if session.current_leaf_id != old_leaf_id {
                    return Err(ProtocolError::InvalidCommand(
                        "session changed while summarizing branch".into(),
                    ));
                }
            }
        }

        let path = {
            let paths = self.session_paths.lock().await;
            paths.get(&session_id).cloned()
        };

        let mut persisted_via_storage = false;
        if let Some(storage) = &self.storage
            && let Some(path) = path.as_ref()
        {
            if let Some(b) = &branch_summary {
                storage.append_entry(path, b, None).map_err(storage_error)?;
                target_id = Some(b.id().to_string());
            }

            let leaf_target = target_id.clone();
            let current_leaf_id = state.session(&session_id)?.current_leaf_id.clone();
            storage
                .navigate(
                    path,
                    current_leaf_id.as_deref(),
                    leaf_target.as_deref(),
                    None,
                )
                .map_err(storage_error)?;

            let persisted = storage.load_by_path(path).map_err(storage_error)?;
            state.insert_session(persisted.state);
            persisted_via_storage = true;
        }

        if !persisted_via_storage {
            let leaf_parent_id = state.session(&session_id)?.current_leaf_id.clone();
            if let Some(b) = &branch_summary {
                state.append_entry(&session_id, b.clone())?;
                target_id = Some(b.id().to_string());
            }
            let leaf_entry = crate::api::SessionTreeEntry::Leaf(crate::api::LeafEntry {
                id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
                parent_id: leaf_parent_id,
                timestamp: now_ms().to_string(),
                target_id: target_id.clone(),
            });
            state.append_entry(&session_id, leaf_entry)?;
        }

        let snapshot = state.snapshot(&session_id)?;
        Ok(vec![
            server_response_ok(
                command_id,
                crate::api::CommandResult::SessionNavigated {
                    session_id: session_id.clone(),
                    old_leaf_id,
                    new_leaf_id: target_id,
                    selected_entry_id: entry_id,
                    editor_text,
                    summary_entry: branch_summary,
                    timestamp: now_ms(),
                },
            ),
            server_response_ok(
                command_id,
                crate::api::CommandResult::SessionOpened {
                    session_id,
                    snapshot,
                    timestamp: now_ms(),
                },
            ),
        ])
    }
}
