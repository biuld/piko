use std::path::PathBuf;

use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::util::{now_ms, storage_error};

use super::helpers::{server_response_ok, session_opened_messages};

impl HostApp {
    pub(super) fn session_open_response(
        state: &mut crate::domain::sessions::HostState,
        command_id: &str,
        session_id: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let interrupt_events = state.finalize_interrupted_turns(&session_id)?;
        let snapshot = state.snapshot(&session_id)?;
        let agents = state.get_agent_list(&session_id);
        Ok(session_opened_messages(
            command_id,
            session_id,
            snapshot,
            agents,
            interrupt_events,
        ))
    }

    pub(crate) async fn apply_session_create(
        &self,
        command_id: &str,
        cwd: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let mut state = self.state.lock().await;
        if let Some(storage) = &self.storage {
            let persisted = storage.create(&cwd).map_err(storage_error)?;
            let session_id = persisted.state.session_id.clone();
            let session_path = persisted.path.clone();
            self.session_paths
                .lock()
                .await
                .insert(session_id.clone(), session_path.clone());
            state.insert_session(persisted.state);
            Ok(vec![server_response_ok(
                command_id,
                crate::api::CommandResult::SessionCreated {
                    session_id,
                    cwd,
                    timestamp: now_ms(),
                },
            )])
        } else {
            Ok(vec![server_response_ok(
                command_id,
                state.create_session(cwd),
            )])
        }
    }

    pub(crate) async fn apply_session_open(
        &self,
        command_id: &str,
        session_id: String,
        session_path: Option<String>,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let mut state = self.state.lock().await;

        // 1. If session_path is provided, load that session directory.
        if let (Some(path_str), Some(storage)) = (session_path, &self.storage) {
            let path = PathBuf::from(path_str);
            let persisted = storage.load_by_path(&path).map_err(|err| match err {
                crate::infra::storage::SessionStorageError::NotFound(_) => {
                    ProtocolError::SessionNotFound(session_id.clone())
                }
                _ => ProtocolError::InvalidCommand(format!("invalid session: {}", err)),
            })?;
            let opened_id = persisted.state.session_id.clone();
            if opened_id != session_id {
                return Err(ProtocolError::InvalidCommand(format!(
                    "session path id mismatch: requested {}, found {}",
                    session_id, opened_id
                )));
            }
            self.session_paths
                .lock()
                .await
                .insert(opened_id.clone(), persisted.path.clone());
            state.insert_session(persisted.state);
            return Self::session_open_response(&mut state, command_id, opened_id);
        }

        // 2. Otherwise, check if it's already in memory.
        if state.has_session(&session_id) {
            return Self::session_open_response(&mut state, command_id, session_id);
        }

        // 3. Search all known sessions.
        if let Some(storage) = &self.storage {
            let all_sessions = storage.list(None).map_err(storage_error)?;
            let exact_match = all_sessions
                .iter()
                .find(|s| s.state.session_id == session_id);
            if let Some(persisted) = exact_match {
                let opened_id = persisted.state.session_id.clone();
                self.session_paths
                    .lock()
                    .await
                    .insert(opened_id.clone(), persisted.path.clone());
                state.insert_session(persisted.state.clone());
                return Self::session_open_response(&mut state, command_id, opened_id);
            }

            // Fallback for prefix matching
            let prefix_matches: Vec<_> = all_sessions
                .iter()
                .filter(|s| s.state.session_id.starts_with(&session_id))
                .collect();
            if prefix_matches.len() > 1 {
                return Err(ProtocolError::InvalidCommand(format!(
                    "ambiguous session ID prefix: {}",
                    session_id
                )));
            } else if prefix_matches.len() == 1 {
                let persisted = prefix_matches[0];
                let opened_id = persisted.state.session_id.clone();
                self.session_paths
                    .lock()
                    .await
                    .insert(opened_id.clone(), persisted.path.clone());
                state.insert_session(persisted.state.clone());
                return Self::session_open_response(&mut state, command_id, opened_id);
            }
        }

        Err(ProtocolError::SessionNotFound(session_id))
    }

    pub(crate) async fn apply_session_list(
        &self,
        command_id: &str,
        scope: crate::api::SessionListScope,
        cwd: Option<String>,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let list_cwd = match scope {
            crate::api::SessionListScope::CurrentFolder => {
                let resolved_cwd = cwd
                    .or_else(|| {
                        std::env::current_dir()
                            .ok()
                            .and_then(|path| path.to_str().map(String::from))
                    })
                    .unwrap_or_else(|| ".".to_string());
                Some(resolved_cwd)
            }
            crate::api::SessionListScope::All => None,
        };

        let sessions = if let Some(storage) = &self.storage {
            storage
                .summaries(list_cwd.as_deref())
                .map_err(storage_error)?
        } else {
            let state = self.state.lock().await;
            let mut list = state.list_sessions();
            if let Some(ref filter_cwd) = list_cwd {
                list.retain(|s| s.cwd == *filter_cwd);
            }
            list
        };

        Ok(vec![server_response_ok(
            command_id,
            crate::api::CommandResult::SessionListed {
                sessions,
                timestamp: now_ms(),
            },
        )])
    }
}
