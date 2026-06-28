use std::path::PathBuf;

use crate::api::{Event, ProtocolError};
use crate::session::load_session;

use super::{HostServer, now_ms, storage_error};

impl HostServer {
    pub(super) async fn apply_session_create(
        &self,
        cwd: String,
    ) -> Result<Vec<Event>, ProtocolError> {
        let mut state = self.state.lock().await;
        if let Some(storage) = &self.storage {
            let persisted = storage.create(&cwd).map_err(storage_error)?;
            let session_id = persisted.state.session_id.clone();
            self.session_paths
                .lock()
                .await
                .insert(session_id.clone(), persisted.path);
            state.insert_session(persisted.state);
            Ok(vec![Event::SessionCreated {
                session_id,
                cwd,
                timestamp: now_ms(),
            }])
        } else {
            Ok(vec![state.create_session(cwd)])
        }
    }

    pub(super) async fn apply_session_open(
        &self,
        session_id: String,
    ) -> Result<Vec<Event>, ProtocolError> {
        let mut state = self.state.lock().await;
        if !state.has_session(&session_id) {
            if let Some(storage) = &self.storage {
                let cwd = std::env::current_dir()
                    .ok()
                    .and_then(|cwd| cwd.to_str().map(str::to_string))
                    .unwrap_or_else(|| ".".to_string());
                let persisted = storage.open(&cwd, &session_id).map_err(storage_error)?;
                let opened_id = persisted.state.session_id.clone();
                self.session_paths
                    .lock()
                    .await
                    .insert(opened_id.clone(), persisted.path);
                state.insert_session(persisted.state);
                let snapshot = state.snapshot(&opened_id)?;
                return Ok(vec![Event::SessionOpened {
                    session_id: opened_id,
                    snapshot,
                    timestamp: now_ms(),
                }]);
            }
            return Err(ProtocolError::SessionNotFound(session_id));
        }
        let snapshot = state.snapshot(&session_id)?;
        Ok(vec![Event::SessionOpened {
            session_id,
            snapshot,
            timestamp: now_ms(),
        }])
    }

    pub(super) async fn apply_session_list(&self) -> Result<Vec<Event>, ProtocolError> {
        let sessions = if let Some(storage) = &self.storage {
            storage.summaries(None).map_err(storage_error)?
        } else {
            self.state.lock().await.list_sessions()
        };
        Ok(vec![Event::SessionListed {
            sessions,
            timestamp: now_ms(),
        }])
    }

    pub(super) async fn apply_session_fork(
        &self,
        session_id: String,
        entry_id: Option<String>,
    ) -> Result<Vec<Event>, ProtocolError> {
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
            .insert(forked_id.clone(), persisted.path);
        state.insert_session(persisted.state);
        let snapshot = state.snapshot(&forked_id)?;
        Ok(vec![
            Event::SessionCreated {
                session_id: forked_id.clone(),
                cwd: snapshot.cwd.clone(),
                timestamp: now_ms(),
            },
            Event::SessionOpened {
                session_id: forked_id,
                snapshot,
                timestamp: now_ms(),
            },
        ])
    }

    pub(super) async fn apply_session_import(
        &self,
        path: String,
    ) -> Result<Vec<Event>, ProtocolError> {
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
            .insert(imported_id.clone(), persisted.path);
        state.insert_session(persisted.state);
        let snapshot = state.snapshot(&imported_id)?;
        Ok(vec![
            Event::SessionCreated {
                session_id: imported_id.clone(),
                cwd: snapshot.cwd.clone(),
                timestamp: now_ms(),
            },
            Event::SessionOpened {
                session_id: imported_id,
                snapshot,
                timestamp: now_ms(),
            },
        ])
    }

    pub(super) async fn apply_session_rename(
        &self,
        session_id: String,
        name: String,
    ) -> Result<Vec<Event>, ProtocolError> {
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
                    .append_session_info(&path, session.current_leaf_id.as_deref(), &name)
                    .map_err(storage_error)?;
            }
        }
        let snapshot = state.snapshot(&session_id)?;
        Ok(vec![Event::SessionOpened {
            session_id,
            snapshot,
            timestamp: now_ms(),
        }])
    }

    pub(super) async fn apply_session_delete(
        &self,
        session_id: String,
    ) -> Result<Vec<Event>, ProtocolError> {
        self.state.lock().await.delete_session(&session_id);
        let path = self.session_paths.lock().await.remove(&session_id);
        if let Some(path) = path {
            let _ = std::fs::remove_file(path);
        }
        self.apply_session_list().await
    }

    pub(super) async fn apply_session_navigate(
        &self,
        session_id: String,
        entry_id: String,
    ) -> Result<Vec<Event>, ProtocolError> {
        let mut state = self.state.lock().await;
        if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                let parent_id = state.session(&session_id)?.current_leaf_id.clone();
                storage
                    .navigate(&path, parent_id.as_deref(), &entry_id)
                    .map_err(storage_error)?;
                let persisted = load_session(&path).map_err(storage_error)?;
                state.insert_session(persisted.state);
            }
        }
        let snapshot = state.snapshot(&session_id)?;
        Ok(vec![Event::SessionOpened {
            session_id,
            snapshot,
            timestamp: now_ms(),
        }])
    }

    pub(super) async fn apply_session_snapshot(
        &self,
        session_id: String,
    ) -> Result<Vec<Event>, ProtocolError> {
        let state = self.state.lock().await;
        let snapshot = state.snapshot(&session_id)?;
        Ok(vec![Event::StateSnapshot {
            session_id,
            snapshot,
            timestamp: now_ms(),
        }])
    }
}
