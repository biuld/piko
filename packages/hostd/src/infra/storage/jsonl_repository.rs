use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use crate::api::{
    AgentTaskResult, AgentTaskState, AgentTaskStatus, CompactionEntry, LeafEntry, Message,
    MessageEntry, ModelChangeEntry, SessionInfoEntry, SessionSummary, SessionTreeEntry, TaskEvent,
    TaskSource, ThinkingLevelChangeEntry,
};
use uuid::Uuid;

use super::jsonl_io::{SessionHeader, append_jsonl, write_header};
use super::types::{JsonlSessionRepository, PersistedSession, SessionStorageError};
use crate::domain::sessions::SessionState;

impl JsonlSessionRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn load_by_path(&self, path: &Path) -> Result<PersistedSession, SessionStorageError> {
        load_session_dir(path)
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

    // ── create / open / list ──

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
        fs::create_dir(&dir).map_err(|source| SessionStorageError::Io {
            path: dir.clone(),
            source,
        })?;
        let main_path = dir.join("main.jsonl");
        let header = SessionHeader {
            kind: "session".to_string(),
            version: 3,
            id: session_id.clone(),
            timestamp: created_at.clone(),
            cwd: cwd.to_string(),
            parent_session: None,
        };
        write_header(&main_path, &header)?;
        // PersistedSession.path is the *directory*
        Ok(PersistedSession {
            state: SessionState::new(session_id.clone(), cwd.to_string()),
            path: dir,
            created_at,
            parent_session_path: None,
        })
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
                if path.is_dir() && path.join("main.jsonl").exists() {
                    match load_session_dir(&path) {
                        Ok(s) => sessions.push(s),
                        Err(_) => continue,
                    }
                }
            }
        }
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(sessions)
    }

    // ── per-agent file resolution ──

    /// Resolve which JSONL file to write to, creating it if needed.
    fn resolve_agent_path(&self, session_dir: &Path, agent_id: Option<&str>) -> PathBuf {
        let aid = agent_id.unwrap_or("main");
        session_dir.join(format!("{aid}.jsonl"))
    }

    // ── append methods (backwards-compatible signatures) ──

    pub fn append_message(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        message: &Message,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let path = self.resolve_agent_path(session_dir, agent_id);
        let entry_id = Uuid::new_v4().to_string()[..8].to_string();
        let entry = SessionTreeEntry::Message(MessageEntry {
            id: entry_id.clone(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            agent_id: agent_id.map(str::to_string),
            message: message.clone(),
        });
        append_jsonl(&path, &entry)?;
        Ok(entry)
    }

    pub fn append_entry(
        &self,
        session_dir: &Path,
        entry: &SessionTreeEntry,
        agent_id: Option<&str>,
    ) -> Result<(), SessionStorageError> {
        let path = self.resolve_agent_path(session_dir, agent_id);
        append_jsonl(&path, entry)
    }

    pub fn apply_task_event(
        &self,
        session_dir: &Path,
        event: &TaskEvent,
    ) -> Result<(), SessionStorageError> {
        let path = session_dir.join("tasks.json");
        let mut sidecar = load_task_sidecar(&path)?;
        sidecar.apply(event);
        let encoded =
            serde_json::to_string_pretty(&sidecar).map_err(|source| SessionStorageError::Json {
                path: path.clone(),
                source,
            })?;
        fs::write(&path, encoded).map_err(|source| SessionStorageError::Io { path, source })
    }

    pub fn append_session_info(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        name: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let path = self.resolve_agent_path(session_dir, agent_id);
        let entry = SessionTreeEntry::SessionInfo(SessionInfoEntry {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            name: Some(name.to_string()),
        });
        append_jsonl(&path, &entry)?;
        Ok(entry)
    }

    pub fn append_config_metadata(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        model_id: Option<&str>,
        provider: Option<&str>,
        thinking_level: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionStorageError> {
        let path = self.resolve_agent_path(session_dir, agent_id);
        let mut entries = Vec::new();
        let mut cur = parent_id.map(str::to_string);
        if let (Some(m), Some(p)) = (model_id, provider) {
            let e = SessionTreeEntry::ModelChange(ModelChangeEntry {
                id: Uuid::new_v4().to_string()[..8].to_string(),
                parent_id: cur.clone(),
                timestamp: timestamp(),
                provider: p.to_string(),
                model_id: m.to_string(),
            });
            cur = Some(e.id().to_string());
            append_jsonl(&path, &e)?;
            entries.push(e);
        }
        if let Some(tl) = thinking_level {
            let e = SessionTreeEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
                id: Uuid::new_v4().to_string()[..8].to_string(),
                parent_id: cur,
                timestamp: timestamp(),
                thinking_level: tl.to_string(),
            });
            append_jsonl(&path, &e)?;
            entries.push(e);
        }
        Ok(entries)
    }

    pub fn append_compaction(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        summary: &str,
        first_kept_entry_id: &str,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let path = self.resolve_agent_path(session_dir, agent_id);
        let entry = SessionTreeEntry::Compaction(CompactionEntry {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            summary: summary.to_string(),
            first_kept_entry_id: first_kept_entry_id.to_string(),
            tokens_before: 0,
            details: None,
            from_hook: None,
        });
        append_jsonl(&path, &entry)?;
        Ok(entry)
    }

    pub fn navigate(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        target_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let path = self.resolve_agent_path(session_dir, agent_id);
        let entry = SessionTreeEntry::Leaf(LeafEntry {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            target_id: target_id.map(str::to_string),
        });
        append_jsonl(&path, &entry)?;
        Ok(entry)
    }

    // ── fork / import ──

    pub fn fork(
        &self,
        _source_id: &str,
        source_dir: &Path,
        entry_id: Option<&str>,
    ) -> Result<PersistedSession, SessionStorageError> {
        let forked_id = Uuid::new_v4().to_string();
        let created_at = timestamp();
        let main_path = source_dir.join("main.jsonl");
        let file = fs::File::open(&main_path).map_err(|e| SessionStorageError::Io {
            path: main_path.clone(),
            source: e,
        })?;
        let lines: Vec<String> = std::io::BufReader::new(file)
            .lines()
            .collect::<Result<_, _>>()
            .map_err(|e| SessionStorageError::Io {
                path: main_path.clone(),
                source: e,
            })?;
        if lines.is_empty() {
            return Err(SessionStorageError::Invalid {
                path: main_path,
                message: "empty".into(),
            });
        }
        let header: SessionHeader =
            serde_json::from_str(&lines[0]).map_err(|e| SessionStorageError::Json {
                path: main_path.clone(),
                source: e,
            })?;

        let mut kept = Vec::new();
        if let Some(tid) = entry_id {
            let mut by_id: std::collections::HashMap<String, (serde_json::Value, Option<String>)> =
                std::collections::HashMap::new();
            for line in &lines[1..] {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
                    && let Some(id) = v.get("id").and_then(|id| id.as_str())
                {
                    let pid = v
                        .get("parentId")
                        .and_then(|p| p.as_str())
                        .map(str::to_string);
                    by_id.insert(id.to_string(), (v.clone(), pid));
                }
            }
            let mut ancestors = std::collections::HashSet::new();
            let mut cur: Option<String> = Some(tid.to_string());
            while let Some(ref cid) = cur {
                ancestors.insert(cid.clone());
                cur = by_id.get(cid).and_then(|(_, p)| p.clone());
            }
            for (_, (v, _)) in by_id.iter().filter(|(id, _)| ancestors.contains(*id)) {
                kept.push(v.clone());
            }
        } else {
            for line in &lines[1..] {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    kept.push(v);
                }
            }
        }

        let cwd_dir = self.session_dir(&header.cwd);
        let forked_dir = cwd_dir.join(format!(
            "{}_{}",
            created_at.replace([':', '.'], "-"),
            forked_id
        ));
        fs::create_dir_all(&forked_dir).map_err(|e| SessionStorageError::Io {
            path: forked_dir.clone(),
            source: e,
        })?;
        let f_main = forked_dir.join("main.jsonl");
        write_header(
            &f_main,
            &SessionHeader {
                kind: "session".to_string(),
                version: 3,
                id: forked_id.clone(),
                timestamp: created_at.clone(),
                cwd: header.cwd.clone(),
                parent_session: Some(source_dir.to_string_lossy().to_string()),
            },
        )?;
        for v in kept {
            append_jsonl(&f_main, &v)?;
        }
        load_session_dir(&forked_dir)
    }

    pub fn import(&self, input_path: &Path) -> Result<PersistedSession, SessionStorageError> {
        if !input_path.exists() {
            return Err(SessionStorageError::NotFound(
                input_path.to_string_lossy().to_string(),
            ));
        }
        if !input_path.is_dir() {
            return Err(SessionStorageError::Invalid {
                path: input_path.to_path_buf(),
                message: "import requires a session directory".into(),
            });
        }
        let src_session = load_session_dir(input_path)?;
        let dest_dir = self.session_dir(&src_session.state.cwd);
        fs::create_dir_all(&dest_dir).map_err(|e| SessionStorageError::Io {
            path: dest_dir.clone(),
            source: e,
        })?;
        let name = input_path.file_name().ok_or(SessionStorageError::Invalid {
            path: input_path.to_path_buf(),
            message: "missing name".into(),
        })?;
        let dest = dest_dir.join(name);
        if dest != input_path {
            copy_dir_all(input_path, &dest).map_err(|e| SessionStorageError::Io {
                path: dest.clone(),
                source: e,
            })?;
        }
        load_session_dir(&dest)
    }

    pub fn summaries(&self, cwd: Option<&str>) -> Result<Vec<SessionSummary>, SessionStorageError> {
        Ok(self
            .list(cwd)?
            .into_iter()
            .map(|s| {
                let session_path = Some(s.path.to_string_lossy().to_string());
                let parent_path = s.parent_session_path.clone();
                s.state
                    .summary(Some(s.created_at.clone()), None, session_path, parent_path)
            })
            .collect())
    }

    fn session_dir(&self, cwd: &str) -> PathBuf {
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

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct TaskSidecar {
    #[serde(flatten)]
    tasks: BTreeMap<String, StoredTask>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StoredTask {
    parent_task_id: Option<String>,
    agent_id: Option<String>,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<i64>,
}

impl TaskSidecar {
    fn apply(&mut self, event: &TaskEvent) {
        match event {
            TaskEvent::Created {
                task_id,
                agent_id,
                parent_task_id,
                source_agent_id,
                prompt,
                timestamp,
                ..
            } => {
                self.tasks.insert(
                    task_id.clone(),
                    StoredTask {
                        parent_task_id: parent_task_id.clone(),
                        agent_id: Some(agent_id.clone()),
                        status: "queued".into(),
                        source_agent_id: source_agent_id.clone(),
                        prompt: Some(prompt.clone()),
                        summary: None,
                        error: None,
                        updated_at: Some(*timestamp),
                    },
                );
            }
            TaskEvent::Started {
                task_id,
                agent_id,
                timestamp,
                ..
            } => {
                let task = self.entry(task_id);
                task.agent_id = Some(agent_id.clone());
                task.status = "running".into();
                task.updated_at = Some(*timestamp);
            }
            TaskEvent::Completed {
                task_id,
                agent_id,
                summary,
                final_status,
                timestamp,
                ..
            } => {
                let task = self.entry(task_id);
                task.agent_id = Some(agent_id.clone());
                task.status = final_status.clone();
                task.summary = Some(summary.clone());
                task.updated_at = Some(*timestamp);
            }
            TaskEvent::Failed {
                task_id,
                agent_id,
                error,
                timestamp,
                ..
            } => {
                let task = self.entry(task_id);
                task.agent_id = Some(agent_id.clone());
                task.status = "failed".into();
                task.error = Some(error.clone());
                task.updated_at = Some(*timestamp);
            }
            TaskEvent::Cancelled {
                task_id,
                agent_id,
                timestamp,
                ..
            } => {
                let task = self.entry(task_id);
                task.agent_id = Some(agent_id.clone());
                task.status = "cancelled".into();
                task.updated_at = Some(*timestamp);
            }
            TaskEvent::Joined {
                task_id,
                parent_task_id,
                timestamp,
                ..
            } => {
                let task = self.entry(task_id);
                task.parent_task_id = Some(parent_task_id.clone());
                task.updated_at = Some(*timestamp);
            }
            TaskEvent::Steered {
                task_id, timestamp, ..
            } => {
                self.entry(task_id).updated_at = Some(*timestamp);
            }
        }
    }

    fn entry(&mut self, task_id: &str) -> &mut StoredTask {
        self.tasks
            .entry(task_id.to_string())
            .or_insert_with(|| StoredTask {
                parent_task_id: None,
                agent_id: None,
                status: "unknown".into(),
                source_agent_id: None,
                prompt: None,
                summary: None,
                error: None,
                updated_at: None,
            })
    }

    fn into_agent_task_states(self) -> HashMap<String, AgentTaskState> {
        self.tasks
            .into_iter()
            .map(|(task_id, task)| {
                let source = match (&task.source_agent_id, &task.parent_task_id) {
                    (Some(agent_id), Some(parent_task_id)) => TaskSource::Agent {
                        agent_id: agent_id.clone(),
                        task_id: parent_task_id.clone(),
                    },
                    _ => TaskSource::User,
                };
                let result = task.summary.clone().map(|summary| AgentTaskResult {
                    summary,
                    artifacts: None,
                });
                let state = AgentTaskState {
                    id: task_id.clone(),
                    target_agent_id: task.agent_id.unwrap_or_else(|| "main".into()),
                    prompt: task.prompt.unwrap_or_default(),
                    source,
                    status: task_status_from_sidecar(&task.status),
                    priority: 0,
                    parent_task_id: task.parent_task_id,
                    result,
                    error: task.error,
                };
                (task_id, state)
            })
            .collect()
    }
}

fn load_task_sidecar(path: &Path) -> Result<TaskSidecar, SessionStorageError> {
    if !path.exists() {
        return Ok(TaskSidecar::default());
    }
    let raw = fs::read_to_string(path).map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&raw).map_err(|source| SessionStorageError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn task_status_from_sidecar(status: &str) -> AgentTaskStatus {
    match status {
        "queued" => AgentTaskStatus::Queued,
        "running" => AgentTaskStatus::Running,
        "completed" => AgentTaskStatus::Completed,
        "failed" => AgentTaskStatus::Failed,
        "cancelled" => AgentTaskStatus::Cancelled,
        _ => AgentTaskStatus::Failed,
    }
}

// ── helpers ──

fn encode_cwd(cwd: &str) -> String {
    format!(
        "--{}--",
        cwd.trim_start_matches(['/', '\\'])
            .replace(['/', '\\', ':'], "-")
    )
}

fn timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!(
        "{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let t = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir_all(&e.path(), &t)?;
        } else {
            fs::copy(e.path(), &t)?;
        }
    }
    Ok(())
}

pub(crate) fn load_session_dir(dir: &Path) -> Result<PersistedSession, SessionStorageError> {
    let main = dir.join("main.jsonl");
    if !main.exists() {
        return Err(SessionStorageError::Invalid {
            path: dir.to_path_buf(),
            message: "missing main.jsonl".into(),
        });
    }
    let (mut state, header) = load_file_state(&main)?;
    for e in fs::read_dir(dir).map_err(|e| SessionStorageError::Io {
        path: dir.to_path_buf(),
        source: e,
    })? {
        let e = e.map_err(|e| SessionStorageError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        let p = e.path();
        if p == main || p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
            continue;
        }
        if let Ok((s, _)) = load_file_state(&p) {
            state.entries.extend(s.entries);
        }
    }
    state.entries.sort_by_key(|e| e.timestamp().to_string());
    state.seq = state.entries.len() as u64;
    state.tasks = load_task_sidecar(&dir.join("tasks.json"))?.into_agent_task_states();
    Ok(PersistedSession {
        state,
        path: dir.to_path_buf(),
        created_at: header.timestamp,
        parent_session_path: header.parent_session,
    })
}

fn load_file_state(path: &Path) -> Result<(SessionState, SessionHeader), SessionStorageError> {
    let f = fs::File::open(path).map_err(|e| SessionStorageError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut lines = std::io::BufReader::new(f).lines();
    let hl = lines
        .next()
        .ok_or(SessionStorageError::Invalid {
            path: path.to_path_buf(),
            message: "missing header".into(),
        })?
        .map_err(|e| SessionStorageError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
    let h: SessionHeader = serde_json::from_str(&hl).map_err(|e| SessionStorageError::Json {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut state = SessionState::new(h.id.clone(), h.cwd.clone());
    for l in lines {
        let l = l.map_err(|e| SessionStorageError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        if l.trim().is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<SessionTreeEntry>(&l) {
            state.current_leaf_id = entry.leaf_target_id().map(str::to_string);
            state.entries.push(entry);
        }
    }
    state.seq = state.entries.len() as u64;
    Ok((state, h))
}
