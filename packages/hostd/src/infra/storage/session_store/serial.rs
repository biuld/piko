//! Per-session durable IO serialization.
//!
//! Durable commands (agent commits, message commits, manifest mutations) must
//! not race on the same session directory, and must not block the Tokio
//! runtime. Callers enter the serial model through [`SessionStore::with_io`]
//! (sync) or [`SessionStore::run_durable`] (async + `spawn_blocking`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use piko_protocol::execution::CommitError;

use super::SessionStore;

fn session_io_locks() -> &'static Mutex<HashMap<PathBuf, Weak<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn io_lock_for(session_dir: &Path) -> Arc<Mutex<()>> {
    let key = session_dir.to_path_buf();
    let mut locks = session_io_locks()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = locks.get(&key).and_then(|weak| weak.upgrade()) {
        return existing;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

impl SessionStore {
    /// Run a durable mutation under the per-session IO lock (sync callers).
    pub fn with_io<R>(&self, f: impl FnOnce() -> R) -> R {
        let _guard = self
            .io
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f()
    }

    /// Run a durable mutation off the async runtime, serialized per session.
    pub async fn run_durable<R, F>(&self, f: F) -> Result<R, CommitError>
    where
        F: FnOnce(&SessionStore) -> Result<R, CommitError> + Send + 'static,
        R: Send + 'static,
    {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.with_io(|| f(&store)))
            .await
            .map_err(|error| CommitError::Failed(format!("durable worker join failed: {error}")))?
    }
}
