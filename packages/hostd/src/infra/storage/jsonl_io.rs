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

/// Load a session from a JSONL file, returning its in-memory state and metadata.
pub fn load_session(path: &Path) -> Result<PersistedSession, SessionStorageError> {
    let file = fs::File::open(path).map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut lines = BufReader::new(file).lines();
    let Some(header_line) = lines.next() else {
        return Err(SessionStorageError::Invalid {
            path: path.to_path_buf(),
            message: "missing session header".to_string(),
        });
    };
    let header_line = header_line.map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let header: SessionHeader =
        serde_json::from_str(&header_line).map_err(|source| SessionStorageError::Json {
            path: path.to_path_buf(),
            source,
        })?;
    if header.kind != "session" || header.version != 3 {
        return Err(SessionStorageError::Invalid {
            path: path.to_path_buf(),
            message: "unsupported session header".to_string(),
        });
    }

    let mut state = SessionState::new(header.id.clone(), header.cwd.clone());

    for line in lines {
        let line = line.map_err(|source| SessionStorageError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }

        let entry = parse_session_entry(&line, path)?;
        state.current_leaf_id = entry.leaf_target_id().map(str::to_string);
        if let SessionTreeEntry::SessionInfo(session_info) = &entry
            && let Some(name) = &session_info.name
        {
            state.name = Some(name.clone());
        }
        state.entries.push(entry);
    }
    state.seq = state.entries.len() as u64;

    Ok(PersistedSession {
        state,
        path: path.to_path_buf(),
        created_at: header.timestamp,
    })
}

fn parse_session_entry(line: &str, path: &Path) -> Result<SessionTreeEntry, SessionStorageError> {
    match serde_json::from_str::<SessionTreeEntry>(line) {
        Ok(entry) => Ok(entry),
        Err(source) => {
            let value: serde_json::Value =
                serde_json::from_str(line).map_err(|source| SessionStorageError::Json {
                    path: path.to_path_buf(),
                    source,
                })?;
            if value.get("type").and_then(|kind| kind.as_str()) == Some("config") {
                parse_legacy_config_entry(value, path)
            } else {
                Err(SessionStorageError::Json {
                    path: path.to_path_buf(),
                    source,
                })
            }
        }
    }
}

fn parse_legacy_config_entry(
    value: serde_json::Value,
    path: &Path,
) -> Result<SessionTreeEntry, SessionStorageError> {
    let id = value
        .get("id")
        .and_then(|id| id.as_str())
        .ok_or_else(|| SessionStorageError::Invalid {
            path: path.to_path_buf(),
            message: "legacy config entry missing id".to_string(),
        })?
        .to_string();
    let parent_id = value
        .get("parentId")
        .and_then(|parent_id| parent_id.as_str())
        .map(str::to_string);
    let timestamp = value
        .get("timestamp")
        .and_then(|timestamp| timestamp.as_str())
        .unwrap_or_default()
        .to_string();

    if let Some(thinking_level) = value.get("thinkingLevel").and_then(|level| level.as_str()) {
        return Ok(SessionTreeEntry::ThinkingLevelChange(
            ThinkingLevelChangeEntry {
                id,
                parent_id,
                timestamp,
                thinking_level: thinking_level.to_string(),
            },
        ));
    }

    let provider = value
        .get("provider")
        .and_then(|provider| provider.as_str())
        .unwrap_or_default()
        .to_string();
    let model_id = value
        .get("model")
        .and_then(|model| model.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(SessionTreeEntry::ModelChange(ModelChangeEntry {
        id,
        parent_id,
        timestamp,
        provider,
        model_id,
    }))
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
