use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::super::session_store::SessionStore;
use super::super::types::{JsonlSessionRepository, PersistedSession, SessionStorageError};
use super::helpers::{encode_cwd, timestamp};
use super::load::load_session_dir;
use crate::api::SessionSummary;

/// Upper bound on parallel session-summary workers. Listing many sessions
/// should not spawn an unbounded number of threads.
const MAX_SUMMARY_WORKERS: usize = 8;

impl JsonlSessionRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn load_by_path(&self, path: &Path) -> Result<PersistedSession, SessionStorageError> {
        load_session_dir(&self.store(path), path)
    }

    pub fn delete(&self, session_dir: &Path) -> Result<(), SessionStorageError> {
        fs::remove_dir_all(session_dir).map_err(|source| SessionStorageError::Io {
            path: session_dir.to_path_buf(),
            source,
        })
    }

    pub fn default_root() -> PathBuf {
        let piko_home = std::env::var_os("PIKO_HOME").map(PathBuf::from);
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        default_root_from(piko_home, PathBuf::from(home))
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
        load_session_dir(&self.store(&dir), &dir)
    }

    pub fn open(
        &self,
        cwd: &str,
        specifier: &str,
    ) -> Result<PersistedSession, SessionStorageError> {
        let path = self
            .resolve_session_dir(Some(cwd), specifier)?
            .ok_or_else(|| SessionStorageError::NotFound(specifier.to_string()))?;
        self.load_by_path(&path)
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
                    match load_session_dir(&self.store(&path), &path) {
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
        let mut paths = Vec::new();
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
                paths.push(path);
            }
        }
        let results = std::thread::scope(|scope| {
            let workers = paths.len().clamp(1, MAX_SUMMARY_WORKERS);
            let chunk_size = paths.len().div_ceil(workers);
            let mut handles = Vec::new();
            for chunk in paths.chunks(chunk_size.max(1)) {
                handles.push(scope.spawn(|| {
                    let items = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        chunk
                            .iter()
                            .map(|path| self.summarize_one(path))
                            .collect::<Vec<_>>()
                    }));
                    items.unwrap_or_else(|_| {
                        chunk
                            .iter()
                            .map(|_| {
                                Err(SessionStorageError::Invalid {
                                    path: PathBuf::new(),
                                    message: "session summary thread panicked".into(),
                                })
                            })
                            .collect()
                    })
                }));
            }
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap_or_default())
                .collect::<Vec<_>>()
        });
        let mut summaries = Vec::new();
        for (path, result) in paths.into_iter().zip(results) {
            match result {
                Ok(summary) => summaries.push(summary),
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
        summaries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(summaries)
    }

    pub(crate) fn store(&self, dir: &Path) -> SessionStore {
        SessionStore::new(dir)
    }

    fn summarize_one(&self, dir: &Path) -> Result<SessionSummary, SessionStorageError> {
        let identity = piko_session_store::SessionStore::inspect(dir).map_err(|error| {
            SessionStorageError::Invalid {
                path: dir.to_path_buf(),
                message: error.to_string(),
            }
        })?;
        let view = piko_session_store::query_catalog(dir).map_err(|error| {
            SessionStorageError::Invalid {
                path: dir.to_path_buf(),
                message: error.to_string(),
            }
        })?;
        let facts = view.facts;
        Ok(SessionSummary {
            session_id: identity.session_id,
            cwd: identity.cwd,
            seq: facts.extra_tree_count + facts.message_count,
            name: facts.name,
            first_message: facts.first_user_message,
            message_count: facts.message_count,
            created_at: Some(identity.created_at.to_string()),
            modified_at: Some(facts.updated_at.to_string()),
            session_path: Some(dir.to_string_lossy().to_string()),
            parent_session_path: None,
            integrity_error: None,
        })
    }

    pub fn resolve_session_dir(
        &self,
        cwd: Option<&str>,
        specifier: &str,
    ) -> Result<Option<PathBuf>, SessionStorageError> {
        let mut matches = Vec::new();
        for path in self.session_identity_dirs(cwd)? {
            let Ok(identity) = piko_session_store::SessionStore::inspect(&path) else {
                continue;
            };
            if identity.session_id == specifier {
                return Ok(Some(path));
            }
            if identity.session_id.starts_with(specifier) {
                matches.push(path);
            }
        }
        Ok(match matches.len() {
            1 => matches.pop(),
            _ => None,
        })
    }

    fn session_identity_dirs(
        &self,
        cwd: Option<&str>,
    ) -> Result<Vec<PathBuf>, SessionStorageError> {
        let dirs = if let Some(cwd) = cwd {
            vec![self.session_dir(cwd)]
        } else {
            self.list_session_dirs()?
        };
        let mut paths = Vec::new();
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
                if path.is_dir() && path.join("session.json").exists() {
                    paths.push(path);
                }
            }
        }
        Ok(paths)
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

fn default_root_from(piko_home: Option<PathBuf>, home: PathBuf) -> PathBuf {
    piko_home
        .unwrap_or_else(|| home.join(".piko"))
        .join("agents")
        .join("sessions")
}

#[cfg(test)]
mod tests {
    use super::default_root_from;
    use std::path::PathBuf;

    #[test]
    fn default_root_uses_unified_agent_home() {
        assert_eq!(
            default_root_from(None, PathBuf::from("home")),
            PathBuf::from("home/.piko/agents/sessions")
        );
        assert_eq!(
            default_root_from(Some(PathBuf::from("install")), PathBuf::from("ignored")),
            PathBuf::from("install/agents/sessions")
        );
    }
}
