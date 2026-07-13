use std::path::PathBuf;

pub use crate::ports::storage_types::{PersistedSession, SessionStorageError};

/// Configuration for session storage location.
#[derive(Debug, Clone)]
pub struct SessionStorageConfig {
    pub root: PathBuf,
}

/// JSONL-backed session repository.
#[derive(Debug, Clone)]
pub struct JsonlSessionRepository {
    pub(crate) root: PathBuf,
}
