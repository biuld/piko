use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use crate::api::{
    CompactionEntry, LeafEntry, Message, MessageEntry, ModelChangeEntry, SessionInfoEntry,
    SessionSummary, SessionTreeEntry, ThinkingLevelChangeEntry,
};
use uuid::Uuid;

use super::io::{SessionHeader, append_jsonl, load_session, write_header};
use super::types::{JsonlSessionRepository, PersistedSession, SessionStorageError};

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
            state: crate::state::SessionState::new(session_id, cwd.to_string()),
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
        message: &Message,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry_id = Uuid::new_v4().to_string()[..8].to_string();
        let entry = SessionTreeEntry::Message(MessageEntry {
            id: entry_id.clone(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            message: message.clone(),
        });
        append_jsonl(session_path, &entry)?;
        Ok(entry)
    }

    pub fn append_entry(
        &self,
        session_path: &Path,
        entry: &SessionTreeEntry,
    ) -> Result<(), SessionStorageError> {
        append_jsonl(session_path, entry)
    }

    pub fn append_session_info(
        &self,
        session_path: &Path,
        parent_id: Option<&str>,
        name: &str,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry_id = Uuid::new_v4().to_string()[..8].to_string();
        let entry = SessionTreeEntry::SessionInfo(SessionInfoEntry {
            id: entry_id,
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            name: Some(name.to_string()),
        });
        append_jsonl(session_path, &entry)?;
        Ok(entry)
    }

    /// Persist a model/thinking config change as a metadata entry in the journal.
    pub fn append_config_metadata(
        &self,
        session_path: &Path,
        parent_id: Option<&str>,
        model_id: Option<&str>,
        provider: Option<&str>,
        thinking_level: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionStorageError> {
        let mut entries = Vec::new();
        let mut current_parent_id = parent_id.map(str::to_string);

        if let (Some(model_id), Some(provider)) = (model_id, provider) {
            let entry = SessionTreeEntry::ModelChange(ModelChangeEntry {
                id: Uuid::new_v4().to_string()[..8].to_string(),
                parent_id: current_parent_id.clone(),
                timestamp: timestamp(),
                provider: provider.to_string(),
                model_id: model_id.to_string(),
            });
            current_parent_id = Some(entry.id().to_string());
            append_jsonl(session_path, &entry)?;
            entries.push(entry);
        }

        if let Some(thinking_level) = thinking_level {
            let entry = SessionTreeEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
                id: Uuid::new_v4().to_string()[..8].to_string(),
                parent_id: current_parent_id,
                timestamp: timestamp(),
                thinking_level: thinking_level.to_string(),
            });
            append_jsonl(session_path, &entry)?;
            entries.push(entry);
        }

        Ok(entries)
    }

    pub fn append_compaction(
        &self,
        session_path: &Path,
        parent_id: Option<&str>,
        summary: &str,
        first_kept_entry_id: &str,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry_id = Uuid::new_v4().to_string()[..8].to_string();
        let entry = SessionTreeEntry::Compaction(CompactionEntry {
            id: entry_id,
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            summary: summary.to_string(),
            first_kept_entry_id: first_kept_entry_id.to_string(),
            tokens_before: 0,
            details: None,
            from_hook: None,
        });
        append_jsonl(session_path, &entry)?;
        Ok(entry)
    }

    pub fn navigate(
        &self,
        session_path: &Path,
        parent_id: Option<&str>,
        target_id: &str,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry_id = Uuid::new_v4().to_string()[..8].to_string();
        let entry = SessionTreeEntry::Leaf(LeafEntry {
            id: entry_id,
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            target_id: Some(target_id.to_string()),
        });
        append_jsonl(session_path, &entry)?;
        Ok(entry)
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
        let reader = std::io::BufReader::new(file);
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
