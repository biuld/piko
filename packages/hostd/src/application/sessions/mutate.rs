use std::path::PathBuf;

use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::util::{now_ms, storage_error};

use super::helpers::{server_response_ok, session_reconciled_message};

impl HostApp {
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
        let persisted = storage
            .fork(&session_id, &source_path, entry_id.as_deref())
            .map_err(storage_error)?;
        let forked_id = persisted.state.session_id.clone();

        let mut state = self.state.lock().await;
        self.session_paths
            .lock()
            .await
            .insert(forked_id.clone(), persisted.path.clone());
        state.insert_session(persisted.state);
        let cwd = state.snapshot(&forked_id)?.cwd.clone();
        let path = persisted.path.clone();
        let mut events = vec![server_response_ok(
            command_id,
            crate::api::CommandResult::SessionCreated {
                session_id: forked_id.clone(),
                cwd,
                timestamp: now_ms(),
            },
        )];
        events.extend(Self::session_open_response(
            &mut state,
            command_id,
            forked_id.clone(),
            Some(&path),
            self.session_store_factory.as_ref(),
        )?);
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
            .map_err(storage_error)?;
        let imported_id = persisted.state.session_id.clone();

        let mut state = self.state.lock().await;
        self.session_paths
            .lock()
            .await
            .insert(imported_id.clone(), persisted.path.clone());
        state.insert_session(persisted.state);
        let cwd = state.snapshot(&imported_id)?.cwd.clone();
        let path = persisted.path.clone();
        let mut events = vec![server_response_ok(
            command_id,
            crate::api::CommandResult::SessionCreated {
                session_id: imported_id.clone(),
                cwd,
                timestamp: now_ms(),
            },
        )];
        events.extend(Self::session_open_response(
            &mut state,
            command_id,
            imported_id.clone(),
            Some(&path),
            self.session_store_factory.as_ref(),
        )?);
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
        let mut state = self.state.lock().await;
        let session = state.session_mut(&session_id)?;
        session.name = Some(name.clone());
        if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                storage
                    .append_session_info(&path, session.current_leaf_id.as_deref(), &name, None)
                    .map_err(storage_error)?;
            }
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
        self.state.lock().await.delete_session(&session_id);
        let path = self.session_paths.lock().await.remove(&session_id);
        if let Some(path) = path {
            let _ = std::fs::remove_file(path);
        }
        self.apply_session_list(command_id, crate::api::SessionListScope::All, None)
            .await
    }

    pub(crate) async fn apply_session_set_label(
        &self,
        command_id: &str,
        session_id: String,
        entry_id: String,
        label: Option<String>,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let mut state = self.state.lock().await;
        if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                let parent_id = state.session(&session_id)?.current_leaf_id.clone();
                let label_entry = crate::api::SessionTreeEntry::Label(piko_protocol::LabelEntry {
                    id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
                    parent_id: parent_id.clone(),
                    timestamp: now_ms().to_string(),
                    target_id: entry_id,
                    label,
                });

                storage
                    .append_entry(&path, &label_entry, None)
                    .map_err(storage_error)?;
                let persisted = storage.load_by_path(&path).map_err(storage_error)?;
                state.insert_session(persisted.state);
            }
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
}
