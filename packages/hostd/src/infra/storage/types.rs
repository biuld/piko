use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::infra::storage::session_store::SessionStore;
pub use crate::ports::storage_types::{PersistedSession, SessionStorageError};
use crate::util::LruMap;
use piko_session_store::JournalFacts;

/// Upper bound on retained full journal handles (each pins writer.lock and
/// the in-memory raw-events cache). Evicted least-recently-used.
pub(crate) const OPEN_STORE_CACHE_CAPACITY: usize = 32;
/// Upper bound on lightweight summary facts.
pub(crate) const FACTS_CACHE_CAPACITY: usize = 256;

/// Journal handle plus the on-disk segment sizes at open time. Listing can
/// cheaply detect external writes/corruption by re-stat'ing the segments and
/// drop the cached handle when they differ.
#[derive(Debug)]
pub(crate) struct CachedJournal {
    pub(crate) store: SessionStore,
    pub(crate) segment_sizes: std::collections::HashMap<PathBuf, u64>,
}

#[derive(Debug)]
pub(crate) struct CachedFacts {
    pub(crate) facts: JournalFacts,
    pub(crate) segment_sizes: std::collections::HashMap<PathBuf, u64>,
}

/// Configuration for session storage location.
#[derive(Debug, Clone)]
pub struct SessionStorageConfig {
    pub root: PathBuf,
}

/// JSONL-backed session repository.
#[derive(Debug, Clone)]
pub struct JsonlSessionRepository {
    pub(crate) root: PathBuf,
    /// Strong references to opened journal handles so repeated list/load
    /// calls reuse the in-memory replay instead of re-reading JSONL.
    /// Capacity-bounded so writer locks and raw-events memory are released
    /// for least-recently-used sessions.
    pub(crate) open_stores: Arc<Mutex<LruMap<PathBuf, CachedJournal>>>,
    /// Lightweight list-summary cache (snapshot + tail), invalidated by
    /// on-disk segment size changes.
    pub(crate) facts_cache: Arc<Mutex<LruMap<PathBuf, CachedFacts>>>,
}
