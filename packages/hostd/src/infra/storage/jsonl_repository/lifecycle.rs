use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::super::session_store::SessionStore;
use super::super::types::{
    CachedFacts, CachedJournal, FACTS_CACHE_CAPACITY, JsonlSessionRepository,
    OPEN_STORE_CACHE_CAPACITY, PersistedSession, SessionStorageError,
};
use super::helpers::{encode_cwd, timestamp};
use super::load::load_session_dir;
use crate::api::SessionSummary;
use crate::util::LruMap;

/// Upper bound on parallel session-summary workers. Listing many sessions
/// should not spawn an unbounded number of threads.
const MAX_SUMMARY_WORKERS: usize = 8;

impl JsonlSessionRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            open_stores: Arc::new(Mutex::new(LruMap::new(OPEN_STORE_CACHE_CAPACITY))),
            facts_cache: Arc::new(Mutex::new(LruMap::new(FACTS_CACHE_CAPACITY))),
        }
    }

    pub fn load_by_path(&self, path: &Path) -> Result<PersistedSession, SessionStorageError> {
        load_session_dir(&self.cached_store(path), path)
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
        load_session_dir(&self.cached_store(&dir), &dir)
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
                    match load_session_dir(&self.cached_store(&path), &path) {
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

    /// Reuse the journal handle for a session directory. Keeps the
    /// in-memory replay (and raw-events cache) alive across list/load calls.
    /// Cheaply re-checks segment sizes so external writes or corruption
    /// invalidate the cached handle instead of being masked by it.
    pub(crate) fn cached_store(&self, dir: &Path) -> SessionStore {
        let mut stores = self
            .open_stores
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(cached) = stores.get(dir)
            && segment_sizes_match(dir, &cached.segment_sizes)
        {
            return cached.store.clone();
        }
        // Stale or missing: drop the old handle (releasing its writer lock)
        // before reopening so this process can re-acquire the lock.
        stores.remove(dir);
        let store = SessionStore::new(dir);
        stores.insert(
            dir.to_path_buf(),
            CachedJournal {
                store: store.clone(),
                segment_sizes: segment_sizes(dir),
            },
        );
        store
    }

    /// Lightweight per-session summary: boundary snapshot + open tail via
    /// `inspect_facts`, falling back to a full journal open for sessions
    /// without a snapshot yet. Cached facts are invalidated when on-disk
    /// segment sizes change.
    fn summarize_one(&self, dir: &Path) -> Result<SessionSummary, SessionStorageError> {
        let identity = piko_session_store::SessionStore::inspect(dir).map_err(|error| {
            SessionStorageError::Invalid {
                path: dir.to_path_buf(),
                message: error.to_string(),
            }
        })?;
        let cached = {
            let mut stores = self
                .facts_cache
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if let Some(cached) = stores.get(dir)
                && segment_sizes_match(dir, &cached.segment_sizes)
            {
                Some(cached.facts.clone())
            } else {
                stores.remove(dir);
                None
            }
        };
        let facts = match cached {
            Some(ref facts) => facts.clone(),
            None => {
                let facts = match piko_session_store::SessionStore::inspect_facts(dir) {
                    Ok(Some(facts)) => Some(facts),
                    Ok(None) => {
                        // No boundary snapshot yet: full open (also seeds the
                        // journal handle cache for later loads).
                        self.cached_store(dir).journal_facts().ok()
                    }
                    Err(error) => {
                        return Err(SessionStorageError::Invalid {
                            path: dir.to_path_buf(),
                            message: error.to_string(),
                        });
                    }
                };
                let Some(facts) = facts else {
                    return Err(SessionStorageError::Invalid {
                        path: dir.to_path_buf(),
                        message: "failed to summarize session".into(),
                    });
                };
                self.facts_cache
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .insert(
                        dir.to_path_buf(),
                        CachedFacts {
                            facts: facts.clone(),
                            segment_sizes: segment_sizes(dir),
                        },
                    );
                facts
            }
        };
        let summary = SessionSummary {
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
        };
        Ok(summary)
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

fn segment_sizes(dir: &Path) -> std::collections::HashMap<PathBuf, u64> {
    let mut sizes = std::collections::HashMap::new();
    if let Ok(entries) = fs::read_dir(dir.join("events")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("jsonl")
                && let Ok(metadata) = entry.metadata()
            {
                sizes.insert(path, metadata.len());
            }
        }
    }
    sizes
}

fn segment_sizes_match(dir: &Path, cached: &std::collections::HashMap<PathBuf, u64>) -> bool {
    segment_sizes(dir) == *cached
}
