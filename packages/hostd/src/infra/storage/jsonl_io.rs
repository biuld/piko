use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::api::{ModelChangeEntry, SessionTreeEntry, ThinkingLevelChangeEntry};
use crate::domain::sessions::SessionState;

use super::types::{PersistedSession, SessionStorageError};

/// Internal JSONL header written as the first line of every session file.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionHeader {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) version: u32,
    pub(crate) id: String,
    pub(crate) timestamp: String,
    pub(crate) cwd: String,
    #[serde(rename = "parentSession", skip_serializing_if = "Option::is_none")]
    pub(crate) parent_session: Option<String>,
}

/// Write the session header as the first line of a new JSONL file.
pub fn write_header(path: &Path, header: &SessionHeader) -> Result<(), SessionStorageError> {
    let mut file = fs::File::create(path).map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let encoded = serde_json::to_string(header).map_err(|source| SessionStorageError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    writeln!(file, "{encoded}").map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Append a single JSON-serializable value as a line to a JSONL file.
pub fn append_jsonl(path: &Path, value: &impl Serialize) -> Result<(), SessionStorageError> {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|source| SessionStorageError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let encoded = serde_json::to_string(value).map_err(|source| SessionStorageError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    writeln!(file, "{encoded}").map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })
}
