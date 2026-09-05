use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("session history publication changed while reading projections; retry the query")]
    InspectionBusy,
    #[error("session not found: {0}")]
    NotFound(PathBuf),
    #[error("session writer is already locked: {0}")]
    WriterLocked(PathBuf),
    #[error("unsupported session schema {found}; reader supports {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("reader upgrade required for event {event_type} v{version}")]
    UpgradeRequired { event_type: String, version: u32 },
    #[error("revision conflict: expected {expected}, current {current}")]
    RevisionConflict { expected: u64, current: u64 },
    #[error("idempotency conflict for {0}")]
    IdempotencyConflict(String),
    #[error("invalid event: {0}")]
    InvalidEvent(String),
    #[error("journal corruption at {path}, line {line}: {message}")]
    Corruption {
        path: PathBuf,
        line: usize,
        message: String,
    },
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

pub type Result<T> = std::result::Result<T, StoreError>;

pub(crate) fn io_error(path: impl Into<PathBuf>, source: std::io::Error) -> StoreError {
    StoreError::Io {
        path: path.into(),
        source,
    }
}
