use std::path::PathBuf;

use crate::domain::sessions::SessionState;

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

/// A loaded session with its in-memory state and file path.
#[derive(Debug, Clone)]
pub struct PersistedSession {
    pub state: SessionState,
    pub path: PathBuf,
    pub created_at: String,
    pub parent_session_path: Option<String>,
}

/// Errors that can occur during session storage operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionStorageError {
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("invalid session {path}: {message}")]
    Invalid { path: PathBuf, message: String },
    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("json error for {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}
