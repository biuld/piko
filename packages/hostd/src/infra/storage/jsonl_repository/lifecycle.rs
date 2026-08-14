use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::super::session_store::SessionStore;
use super::super::types::{JsonlSessionRepository, PersistedSession, SessionStorageError};
use super::helpers::{encode_cwd, timestamp};
use super::load::load_session_dir;
use crate::api::SessionSummary;

impl JsonlSessionRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn load_by_path(&self, path: &Path) -> Result<PersistedSession, SessionStorageError> {
        load_session_dir(path)
    }

    pub fn delete(&self, session_dir: &Path) -> Result<(), SessionStorageError> {
        fs::remove_dir_all(session_dir).map_err(|source| SessionStorageError::Io {
            path: session_dir.to_path_buf(),
            source,
        })
    }

    pub fn default_root() -> PathBuf {
        if let Some(root) = std::env::var_os("PIKO_HOME") {
            return PathBuf::from(root).join("agent").join("sessions");
        }
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home)
            .join(".piko")
            .join("agent")
            .join("sessions")
    }
    pub fn create(&self, cwd: &str) -> Result<PersistedSession, SessionStorageError> {
        let session_id = Uuid::new_v4().to_string();
        let created_at = timestamp();
        let dir = self.session_dir(cwd);
        fs::create_dir_all(&dir).map_err(|source| SessionStorageError::Io {
            path: dir.clone(),
            source,
        })?;
        let dir = dir.join(format!(
            "{}_{}",
            created_at.replace([':', '.'], "-"),
            session_id
        ));
        SessionStore::create_session(
            dir.clone(),
            session_id,
            cwd.to_string(),
            created_at.parse().unwrap_or_default(),
        )?;
        load_session_dir(&dir)
    }

    pub fn open(
        &self,
        cwd: &str,
        specifier: &str,
    ) -> Result<PersistedSession, SessionStorageError> {
        let sessions = self.list(Some(cwd))?;
        sessions
            .into_iter()
            .find(|s| s.state.session_id == specifier || s.state.session_id.starts_with(specifier))
            .ok_or_else(|| SessionStorageError::NotFound(specifier.to_string()))
    }

    pub fn list(&self, cwd: Option<&str>) -> Result<Vec<PersistedSession>, SessionStorageError> {
        let dirs = if let Some(c) = cwd {
            vec![self.session_dir(c)]
        } else {
            self.list_session_dirs()?
        };
        let mut sessions = Vec::new();
        for dir in dirs {
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir).map_err(|e| SessionStorageError::Io {
                path: dir.clone(),
                source: e,
            })? {
                let entry = entry.map_err(|e| SessionStorageError::Io {
                    path: dir.clone(),
                    source: e,
                })?;
                let path = entry.path();
                if path.is_dir() && path.join("session.json").exists() {
                    match load_session_dir(&path) {
                        Ok(s) => sessions.push(s),
                        Err(_) => continue,
                    }
                }
            }
        }
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sessions)
    }
    pub fn summaries(&self, cwd: Option<&str>) -> Result<Vec<SessionSummary>, SessionStorageError> {
        let dirs = if let Some(cwd) = cwd {
            vec![self.session_dir(cwd)]
        } else {
            self.list_session_dirs()?
        };
        let mut summaries = Vec::new();
        for dir in dirs {
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir).map_err(|source| SessionStorageError::Io {
                path: dir.clone(),
                source,
            })? {
                let path = entry
                    .map_err(|source| SessionStorageError::Io {
                        path: dir.clone(),
                        source,
                    })?
                    .path();
                if !path.is_dir() || !path.join("session.json").exists() {
                    continue;
                }
                match load_session_dir(&path) {
                    Ok(session) => {
                        let session_path = Some(session.path.to_string_lossy().to_string());
                        let parent_path = session.parent_session_path.clone();
                        summaries.push(session.state.summary(
                            Some(session.created_at),
                            None,
                            session_path,
                            parent_path,
                        ));
                    }
                    Err(error) => {
                        // Schema-v4 identity remains listable even if journal
                        // replay fails. Unsupported older schemas are omitted.
                        let Ok(identity) = piko_session_store::SessionStore::inspect(&path) else {
                            continue;
                        };
                        summaries.push(SessionSummary {
                            session_id: identity.session_id,
                            cwd: identity.cwd,
                            seq: 0,
                            name: None,
                            first_message: None,
                            message_count: 0,
                            created_at: Some(identity.created_at.to_string()),
                            modified_at: None,
                            session_path: Some(path.to_string_lossy().to_string()),
                            parent_session_path: None,
                            integrity_error: Some(error.to_string()),
                        });
                    }
                }
            }
        }
        summaries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(summaries)
    }

    pub(super) fn session_dir(&self, cwd: &str) -> PathBuf {
        self.root.join(encode_cwd(cwd))
    }

    fn list_session_dirs(&self) -> Result<Vec<PathBuf>, SessionStorageError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut d = Vec::new();
        for e in fs::read_dir(&self.root).map_err(|e| SessionStorageError::Io {
            path: self.root.clone(),
            source: e,
        })? {
            let e = e.map_err(|e| SessionStorageError::Io {
                path: self.root.clone(),
                source: e,
            })?;
            if e.path().is_dir() {
                d.push(e.path());
            }
        }
        Ok(d)
    }
}
