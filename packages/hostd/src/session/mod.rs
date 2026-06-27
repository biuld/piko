use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::api::{MessageRole, SessionMessage, SessionSummary};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::SessionState;

#[derive(Debug, Clone)]
pub struct SessionStorageConfig {
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct JsonlSessionRepository {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PersistedSession {
    pub state: SessionState,
    pub path: PathBuf,
    pub created_at: String,
}

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

impl JsonlSessionRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_root() -> PathBuf {
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
        let path = dir.join(format!(
            "{}_{}.jsonl",
            created_at.replace([':', '.'], "-"),
            session_id
        ));
        let header = SessionHeader {
            kind: "session".to_string(),
            version: 3,
            id: session_id.clone(),
            timestamp: created_at.clone(),
            cwd: cwd.to_string(),
            parent_session: None,
        };
        write_header(&path, &header)?;
        Ok(PersistedSession {
            state: SessionState::new(session_id, cwd.to_string()),
            path,
            created_at,
        })
    }

    pub fn open(
        &self,
        cwd: &str,
        specifier: &str,
    ) -> Result<PersistedSession, SessionStorageError> {
        let sessions = self.list(Some(cwd))?;
        let Some(summary) = sessions.into_iter().find(|session| {
            session.state.session_id == specifier || session.state.session_id.starts_with(specifier)
        }) else {
            return Err(SessionStorageError::NotFound(specifier.to_string()));
        };
        Ok(summary)
    }

    pub fn list(&self, cwd: Option<&str>) -> Result<Vec<PersistedSession>, SessionStorageError> {
        let dirs = if let Some(cwd) = cwd {
            vec![self.session_dir(cwd)]
        } else {
            self.list_session_dirs()?
        };
        let mut sessions = Vec::new();
        for dir in dirs {
            if !dir.exists() {
                continue;
            }
            let entries = fs::read_dir(&dir).map_err(|source| SessionStorageError::Io {
                path: dir.clone(),
                source,
            })?;
            for entry in entries {
                let entry = entry.map_err(|source| SessionStorageError::Io {
                    path: dir.clone(),
                    source,
                })?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                    continue;
                }
                sessions.push(load_session(&path)?);
            }
        }
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sessions)
    }

    pub fn append_message(
        &self,
        session_path: &Path,
        parent_id: Option<&str>,
        message: &SessionMessage,
    ) -> Result<String, SessionStorageError> {
        let entry_id = Uuid::new_v4().to_string()[..8].to_string();
        let entry = MessageEntry {
            kind: "message".to_string(),
            id: entry_id.clone(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            message: PersistedMessage {
                role: match message.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::ToolResult => "tool_result".to_string(),
                    MessageRole::Tool => "tool".to_string(),
                    MessageRole::System => "system".to_string(),
                },
                content: serde_json::Value::String(message.text.clone()),
            },
        };
        append_jsonl(session_path, &entry)?;
        Ok(entry_id)
    }

    pub fn append_session_info(
        &self,
        session_path: &Path,
        parent_id: Option<&str>,
        name: &str,
    ) -> Result<String, SessionStorageError> {
        let entry_id = Uuid::new_v4().to_string()[..8].to_string();
        let entry = serde_json::json!({
            "type": "session_info",
            "id": entry_id.clone(),
            "parentId": parent_id,
            "timestamp": timestamp(),
            "name": name,
        });
        append_jsonl(session_path, &entry)?;
        Ok(entry_id)
    }

    /// Persist a model/thinking config change as a metadata entry in the journal.
    pub fn append_config_metadata(
        &self,
        session_path: &Path,
        parent_id: Option<&str>,
        model_id: Option<&str>,
        provider: Option<&str>,
        thinking_level: Option<&str>,
    ) -> Result<String, SessionStorageError> {
        let entry_id = Uuid::new_v4().to_string()[..8].to_string();
        let mut entry = serde_json::json!({
            "type": "config",
            "id": entry_id.clone(),
            "parentId": parent_id,
            "timestamp": timestamp(),
        });
        let config = entry.as_object_mut().unwrap();
        if let Some(m) = model_id {
            config.insert("model".into(), serde_json::Value::String(m.to_string()));
        }
        if let Some(p) = provider {
            config.insert("provider".into(), serde_json::Value::String(p.to_string()));
        }
        if let Some(t) = thinking_level {
            config.insert(
                "thinkingLevel".into(),
                serde_json::Value::String(t.to_string()),
            );
        }
        append_jsonl(session_path, &entry)?;
        Ok(entry_id)
    }

    pub fn navigate(
        &self,
        session_path: &Path,
        parent_id: Option<&str>,
        target_id: &str,
    ) -> Result<String, SessionStorageError> {
        let entry_id = Uuid::new_v4().to_string()[..8].to_string();
        let entry = serde_json::json!({
            "type": "leaf",
            "id": entry_id.clone(),
            "parentId": parent_id,
            "timestamp": timestamp(),
            "targetId": target_id,
        });
        append_jsonl(session_path, &entry)?;
        Ok(entry_id)
    }

    pub fn fork(
        &self,
        _source_id: &str,
        source_path: &Path,
        entry_id: Option<&str>,
    ) -> Result<PersistedSession, SessionStorageError> {
        let forked_session_id = Uuid::new_v4().to_string();
        let created_at = timestamp();

        // Read entries from source path
        let file = fs::File::open(source_path).map_err(|source| SessionStorageError::Io {
            path: source_path.to_path_buf(),
            source,
        })?;
        let reader = BufReader::new(file);
        let mut lines = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|source| SessionStorageError::Io {
                path: source_path.to_path_buf(),
                source,
            })?;
            lines.push(line);
        }

        if lines.is_empty() {
            return Err(SessionStorageError::Invalid {
                path: source_path.to_path_buf(),
                message: "empty session file".to_string(),
            });
        }

        // Parse the header
        let header: SessionHeader =
            serde_json::from_str(&lines[0]).map_err(|source| SessionStorageError::Json {
                path: source_path.to_path_buf(),
                source,
            })?;

        // Find ancestor entries up to target entry_id (if specified)
        let mut kept_entries = Vec::new();
        if let Some(target_id) = entry_id {
            // Build tree representation or map of entries to find ancestry
            let mut entries_by_id = std::collections::HashMap::new();
            let mut all_parsed = Vec::new();
            for (idx, line) in lines.iter().enumerate().skip(1) {
                if line.trim().is_empty() {
                    continue;
                }
                let val: serde_json::Value =
                    serde_json::from_str(line).map_err(|source| SessionStorageError::Json {
                        path: source_path.to_path_buf(),
                        source,
                    })?;
                if let Some(id) = val.get("id").and_then(|id| id.as_str()) {
                    let parent_id = val
                        .get("parentId")
                        .and_then(|pid| pid.as_str())
                        .map(str::to_string);
                    entries_by_id.insert(id.to_string(), (val.clone(), parent_id, idx));
                    all_parsed.push((id.to_string(), val));
                }
            }

            // Build set of ancestor IDs starting from target_id
            let mut ancestor_ids = std::collections::HashSet::new();
            let mut current = Some(target_id.to_string());
            while let Some(curr_id) = current {
                ancestor_ids.insert(curr_id.clone());
                if let Some((_, parent_id, _)) = entries_by_id.get(&curr_id) {
                    current = parent_id.clone();
                } else {
                    current = None;
                }
            }

            // Keep entries that are in the ancestor set
            for (id, val) in all_parsed {
                if ancestor_ids.contains(&id) {
                    kept_entries.push(val);
                }
            }
        } else {
            // Keep all entries
            for line in lines.iter().skip(1) {
                if line.trim().is_empty() {
                    continue;
                }
                let val: serde_json::Value =
                    serde_json::from_str(line).map_err(|source| SessionStorageError::Json {
                        path: source_path.to_path_buf(),
                        source,
                    })?;
                kept_entries.push(val);
            }
        }

        let dir = self.session_dir(&header.cwd);
        fs::create_dir_all(&dir).map_err(|source| SessionStorageError::Io {
            path: dir.clone(),
            source,
        })?;

        let path = dir.join(format!(
            "{}_{}.jsonl",
            created_at.replace([':', '.'], "-"),
            forked_session_id
        ));

        let forked_header = SessionHeader {
            kind: "session".to_string(),
            version: 3,
            id: forked_session_id.clone(),
            timestamp: created_at.clone(),
            cwd: header.cwd.clone(),
            parent_session: Some(source_path.to_string_lossy().to_string()),
        };
        write_header(&path, &forked_header)?;

        // Write kept entries to new file
        for entry in kept_entries {
            append_jsonl(&path, &entry)?;
        }

        // Load the new session state
        load_session(&path)
    }

    pub fn import(&self, input_path: &Path) -> Result<PersistedSession, SessionStorageError> {
        if !input_path.exists() {
            return Err(SessionStorageError::NotFound(
                input_path.to_string_lossy().to_string(),
            ));
        }
        let temp_session = load_session(input_path)?;

        let destination_dir = self.session_dir(&temp_session.state.cwd);
        fs::create_dir_all(&destination_dir).map_err(|source| SessionStorageError::Io {
            path: destination_dir.clone(),
            source,
        })?;

        let filename = input_path
            .file_name()
            .ok_or_else(|| SessionStorageError::Invalid {
                path: input_path.to_path_buf(),
                message: "missing filename".to_string(),
            })?;
        let destination_path = destination_dir.join(filename);
        if destination_path != input_path {
            fs::copy(input_path, &destination_path).map_err(|source| SessionStorageError::Io {
                path: destination_path.clone(),
                source,
            })?;
        }

        load_session(&destination_path)
    }

    pub fn summaries(&self, cwd: Option<&str>) -> Result<Vec<SessionSummary>, SessionStorageError> {
        Ok(self
            .list(cwd)?
            .into_iter()
            .map(|session| session.state.summary())
            .collect())
    }

    fn session_dir(&self, cwd: &str) -> PathBuf {
        self.root.join(encode_cwd(cwd))
    }

    fn list_session_dirs(&self) -> Result<Vec<PathBuf>, SessionStorageError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let entries = fs::read_dir(&self.root).map_err(|source| SessionStorageError::Io {
            path: self.root.clone(),
            source,
        })?;
        let mut dirs = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| SessionStorageError::Io {
                path: self.root.clone(),
                source,
            })?;
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            }
        }
        Ok(dirs)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionHeader {
    #[serde(rename = "type")]
    kind: String,
    version: u32,
    id: String,
    timestamp: String,
    cwd: String,
    #[serde(rename = "parentSession", skip_serializing_if = "Option::is_none")]
    parent_session: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MessageEntry {
    #[serde(rename = "type")]
    kind: String,
    id: String,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    timestamp: String,
    message: PersistedMessage,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedMessage {
    role: String,
    content: serde_json::Value,
}

pub(crate) fn load_session(path: &Path) -> Result<PersistedSession, SessionStorageError> {
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

    // Parse all entries in the file
    let mut all_entries = Vec::new();
    let mut entries_by_id = std::collections::HashMap::new();

    struct ParsedEntry {
        id: String,
        parent_id: Option<String>,
        kind: String,
        role: Option<String>,
        content: Option<serde_json::Value>,
        target_id: Option<String>,
        name: Option<String>,
    }

    for line in lines {
        let line = line.map_err(|source| SessionStorageError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }

        let value: serde_json::Value =
            serde_json::from_str(&line).map_err(|source| SessionStorageError::Json {
                path: path.to_path_buf(),
                source,
            })?;

        let kind = value
            .get("type")
            .and_then(|k| k.as_str())
            .unwrap_or("")
            .to_string();
        let id = value
            .get("id")
            .and_then(|id| id.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let parent_id = value
            .get("parentId")
            .and_then(|pid| pid.as_str())
            .map(str::to_string);
        let role = value
            .get("message")
            .and_then(|m| m.get("role"))
            .and_then(|r| r.as_str())
            .map(str::to_string);
        let content = value.get("message").and_then(|m| m.get("content")).cloned();
        let target_id = value
            .get("targetId")
            .and_then(|tid| tid.as_str())
            .map(str::to_string);
        let name = value
            .get("name")
            .and_then(|n| n.as_str())
            .map(str::to_string);

        let parsed = ParsedEntry {
            id: id.clone(),
            parent_id,
            kind,
            role,
            content,
            target_id,
            name,
        };
        entries_by_id.insert(id, all_entries.len());
        all_entries.push(parsed);
    }

    // Find active leaf ID
    let mut current_leaf_id: Option<String> = None;
    for entry in &all_entries {
        if entry.kind == "leaf" {
            current_leaf_id = entry.target_id.clone();
        } else {
            current_leaf_id = Some(entry.id.clone());
        }
    }

    // Build ancestor IDs set from current_leaf_id
    let mut ancestor_ids = std::collections::HashSet::new();
    let mut curr = current_leaf_id.clone();
    while let Some(id) = curr {
        ancestor_ids.insert(id.clone());
        if let Some(&idx) = entries_by_id.get(&id) {
            curr = all_entries[idx].parent_id.clone();
        } else {
            curr = None;
        }
    }

    let mut state = SessionState::new(header.id.clone(), header.cwd.clone());
    state.current_leaf_id = current_leaf_id;

    // Filter and apply entries that are part of the active branch, in original order
    for entry in all_entries {
        if !ancestor_ids.contains(&entry.id) {
            continue;
        }

        if entry.kind == "message" {
            let role = match entry.role.as_deref() {
                Some("user") => MessageRole::User,
                Some("assistant") => MessageRole::Assistant,
                Some("tool") | Some("toolResult") => MessageRole::Tool,
                _ => MessageRole::System,
            };
            let text = entry
                .content
                .as_ref()
                .map(message_content_to_text)
                .unwrap_or_default();
            state.messages.push(SessionMessage {
                id: entry.id,
                role,
                text,
            });
        } else if entry.kind == "session_info"
            && let Some(name) = entry.name
        {
            state.name = Some(name);
        }
        // config metadata entries are informational — not needed for transcript replay
    }

    Ok(PersistedSession {
        state,
        path: path.to_path_buf(),
        created_at: header.timestamp,
    })
}

fn message_content_to_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                block
                    .get("text")
                    .and_then(|text| text.as_str())
                    .or_else(|| block.get("content").and_then(|text| text.as_str()))
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn write_header(path: &Path, header: &SessionHeader) -> Result<(), SessionStorageError> {
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

fn append_jsonl(path: &Path, value: &impl Serialize) -> Result<(), SessionStorageError> {
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

fn encode_cwd(cwd: &str) -> String {
    let normalized = cwd
        .trim_start_matches(['/', '\\'])
        .replace(['/', '\\', ':'], "-");
    format!("--{normalized}--")
}

fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{millis}")
}
