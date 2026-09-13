use std::path::{Path, PathBuf};

use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::util::{now_ms, storage_error};

use super::helpers::{server_response_ok, session_reconciled_message};

impl HostApp {
    async fn resolve_session_storage_path(
        &self,
        session_id: &str,
    ) -> Result<PathBuf, ProtocolError> {
        if let Some(path) = self.session_paths.lock().await.get(session_id).cloned() {
            return Ok(path);
        }
        let Some(storage) = &self.storage else {
            return Err(ProtocolError::SessionNotFound(session_id.into()));
        };
        let summaries = storage.summaries(None).await.map_err(storage_error)?;
        let Some(summary) = summaries.iter().find(|s| s.session_id == session_id) else {
            return Err(ProtocolError::SessionNotFound(session_id.into()));
        };
        summary
            .session_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or_else(|| ProtocolError::SessionNotFound(session_id.into()))
    }

    async fn ensure_session_hydrated_for_mutate(
        &self,
        session_id: &str,
        path: &Path,
    ) -> Result<(), ProtocolError> {
        {
            let state = self.state.lock().await;
            if state.has_session(session_id) {
                return Ok(());
            }
        }
        let Some(storage) = &self.storage else {
            return Err(ProtocolError::SessionNotFound(session_id.into()));
        };
        let persisted = storage.load_by_path(path).await.map_err(storage_error)?;
        self.session_paths
            .lock()
            .await
            .insert(session_id.to_string(), path.to_path_buf());
        let mut state = self.state.lock().await;
        if !state.has_session(session_id) {
            state.insert_session(persisted.state);
        }
        Ok(())
    }

    pub(crate) async fn apply_session_fork(
        &self,
        command_id: &str,
        session_id: String,
        entry_id: Option<String>,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let Some(storage) = &self.storage else {
            return Err(ProtocolError::InvalidCommand(
                "session_fork requires persistent storage".into(),
            ));
        };
        let source_path = {
            let paths = self.session_paths.lock().await;
            paths.get(&session_id).cloned()
        };
        let Some(source_path) = source_path else {
            return Err(ProtocolError::SessionNotFound(session_id));
        };
        if self.session_has_active_work(&session_id).await? {
            return Err(ProtocolError::ActiveTurnExists(session_id.clone()));
        }
        let persisted = storage
            .fork(&session_id, &source_path, entry_id.as_deref())
            .await
            .map_err(storage_error)?;
        let forked_id = persisted.state.session_id.clone();
        let reconciliation = Self::prepare_session_open(
            &forked_id,
            Some(&persisted.path),
            self.session_store_factory.as_ref(),
            false,
        )
        .await?;

        let mut state = self.state.lock().await;
        self.session_paths
            .lock()
            .await
            .insert(forked_id.clone(), persisted.path.clone());
        state.insert_session(persisted.state);
        let mut events =
            Self::session_open_response(&mut state, command_id, forked_id.clone(), reconciliation)?;
        drop(state);
        events = self.enrich_reconcile_messages(&forked_id, events).await;
        Ok(events)
    }

    pub(crate) async fn apply_session_import(
        &self,
        command_id: &str,
        path: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let Some(storage) = &self.storage else {
            return Err(ProtocolError::InvalidCommand(
                "session_import requires persistent storage".into(),
            ));
        };
        let persisted = storage
            .import(&PathBuf::from(path))
            .await
            .map_err(storage_error)?;
        let imported_id = persisted.state.session_id.clone();
        let reconciliation = Self::prepare_session_open(
            &imported_id,
            Some(&persisted.path),
            self.session_store_factory.as_ref(),
            false,
        )
        .await?;

        let mut state = self.state.lock().await;
        self.session_paths
            .lock()
            .await
            .insert(imported_id.clone(), persisted.path.clone());
        state.insert_session(persisted.state);
        let mut events = Self::session_open_response(
            &mut state,
            command_id,
            imported_id.clone(),
            reconciliation,
        )?;
        drop(state);
        events = self.enrich_reconcile_messages(&imported_id, events).await;
        Ok(events)
    }

    pub(crate) async fn apply_session_rename(
        &self,
        command_id: &str,
        session_id: String,
        name: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let path = self.resolve_session_storage_path(&session_id).await?;
        self.ensure_session_hydrated_for_mutate(&session_id, &path)
            .await?;

        let parent_id = self
            .state
            .lock()
            .await
            .session(&session_id)?
            .current_leaf_id
            .clone();
        let durable_entry = if let Some(storage) = &self.storage {
            Some(
                storage
                    .append_session_info(&path, parent_id.as_deref(), &name, None)
                    .await
                    .map_err(storage_error)?,
            )
        } else {
            None
        };
        let mut state = self.state.lock().await;
        if let Some(entry) = durable_entry {
            state.append_entry(&session_id, entry)?;
        } else {
            state.session_mut(&session_id)?.name = Some(name.clone());
        }
        drop(state);
        let (snapshot, agents) = self.session_view(&session_id).await?;
        Ok(vec![
            server_response_ok(command_id, crate::api::CommandResult::Empty),
            session_reconciled_message(
                session_id,
                piko_protocol::ReconcileReason::ExplicitRefresh,
                snapshot,
                agents,
            ),
        ])
    }

    pub(crate) async fn apply_session_delete(
        &self,
        command_id: &str,
        session_id: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        // P1-3: deleting a session with live Agent work would strand the
        // running orchd actor — it would keep committing messages and the
        // terminal fact to a deleted journal, and the client would observe
        // stale turn events after `SessionCleared`. Fork/navigate apply the
        // same guard.
        let path = self.resolve_session_storage_path(&session_id).await.ok();
        let mapped = self.session_paths.lock().await.get(&session_id).cloned();
        let path = path.or(mapped);
        // A listed durable session need not have been opened in this process.
        // Hydrate it before consulting the active-work guard, whose in-memory
        // lookup deliberately rejects unknown session identities.
        if let Some(path) = path.as_deref() {
            self.ensure_session_hydrated_for_mutate(&session_id, path)
                .await?;
        }
        if self.session_has_active_work(&session_id).await? {
            return Err(ProtocolError::ActiveTurnExists(session_id.clone()));
        }
        if let Some(path) = path {
            if let Some(storage) = &self.storage {
                storage.delete(&path).await.map_err(storage_error)?;
            } else {
                self.session_store_factory
                    .delete(&path)
                    .await
                    .map_err(storage_error)?;
            }
        }
        self.state.lock().await.delete_session(&session_id);
        self.session_paths.lock().await.remove(&session_id);
        Ok(vec![
            server_response_ok(command_id, crate::api::CommandResult::Empty),
            ServerMessage::SessionCleared(piko_protocol::SessionClearedEvent {
                previous_session_id: session_id,
            }),
        ])
    }

    pub(crate) async fn apply_session_set_label(
        &self,
        command_id: &str,
        session_id: String,
        entry_id: String,
        label: Option<String>,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                let parent_id = self
                    .state
                    .lock()
                    .await
                    .session(&session_id)?
                    .current_leaf_id
                    .clone();
                let label_entry = crate::api::SessionTreeEntry::Label(piko_protocol::LabelEntry {
                    id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
                    parent_id: parent_id.clone(),
                    timestamp: now_ms().to_string(),
                    target_id: entry_id,
                    label,
                });

                storage
                    .append_entry(&path, &label_entry, None)
                    .await
                    .map_err(storage_error)?;
                let persisted = storage.load_by_path(&path).await.map_err(storage_error)?;
                self.state.lock().await.insert_session(persisted.state);
            }
        }

        let (snapshot, agents) = self.session_view(&session_id).await?;
        Ok(vec![
            server_response_ok(command_id, crate::api::CommandResult::Empty),
            session_reconciled_message(
                session_id,
                piko_protocol::ReconcileReason::ExplicitRefresh,
                snapshot,
                agents,
            ),
        ])
    }
}
