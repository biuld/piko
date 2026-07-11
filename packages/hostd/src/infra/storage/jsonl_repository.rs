use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use crate::api::{
    AgentInfo, AgentStatus, AgentTaskResult, AgentTaskState, AgentTaskStatus, CompactionEntry,
    LeafEntry, Message, ModelChangeEntry, ServerMessage, SessionInfoEntry, SessionSummary,
    SessionTreeEntry, TaskEvent, TaskSource, ThinkingLevelChangeEntry,
};
use uuid::Uuid;

use super::jsonl_io::SessionHeader;
use super::recovery::{agent_task_state_from_recovered, transcript_entries_from_recovered};
use super::task_repository::{SESSION_SCHEMA_VERSION, TaskRepository, TaskShardHeader};
use super::types::{JsonlSessionRepository, PersistedSession, SessionStorageError};
use crate::domain::sessions::SessionState;
use crate::domain::sessions::state::AgentViewState;

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
        TaskRepository::create_session(
            dir.clone(),
            session_id.clone(),
            cwd.to_string(),
            created_at.parse().unwrap_or_default(),
        )?;
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
                if path.is_dir() && path.join("session.json").exists() {
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

    pub fn append_entry(
        &self,
        session_dir: &Path,
        entry: &SessionTreeEntry,
        _agent_id: Option<&str>,
    ) -> Result<(), SessionStorageError> {
        let repository = TaskRepository::new(session_dir);
        match entry {
            SessionTreeEntry::Message(message) => {
                let task_id = &message.task_id;
                let agent_id = &message.agent_id;
                let recovered =
                    repository.load_task(&repository.load_manifest()?.session_id, task_id)?;
                repository
                    .commit_message(orchd_api::MessageCommit {
                        session_id: repository.load_manifest()?.session_id,
                        task_id: task_id.clone(),
                        agent_id: agent_id.clone(),
                        work_id: message.work_id.clone(),
                        task_seq: message.task_seq,
                        message_id: message.id.clone(),
                        parent_message_id: recovered.head_message_id,
                        message: message.message.clone(),
                        committed_at: message.timestamp.parse().unwrap_or_default(),
                    })
                    .map_err(persist_storage_error)?;
                Ok(())
            }
            SessionTreeEntry::ToolCall(tool) => {
                let (Some(task_id), Some(agent_id)) = (&tool.task_id, &tool.agent_id) else {
                    return Err(SessionStorageError::Invalid {
                        path: session_dir.to_path_buf(),
                        message: "tool entry requires task_id and agent_id".into(),
                    });
                };
                let manifest = repository.load_manifest()?;
                let recovered = repository.load_task(&manifest.session_id, task_id)?;
                repository
                    .commit_message(orchd_api::MessageCommit {
                        session_id: manifest.session_id,
                        task_id: task_id.clone(),
                        agent_id: agent_id.clone(),
                        work_id: task_id.clone(),
                        task_seq: recovered.last_task_seq + 1,
                        message_id: tool.id.clone(),
                        parent_message_id: recovered.head_message_id,
                        message: Message::ToolCall {
                            id: tool.tool_call_id.clone(),
                            name: tool.tool_name.clone(),
                            arguments: tool.arguments.clone(),
                            model: tool.model.clone(),
                            provider: tool.provider.clone(),
                            timestamp: tool.timestamp.parse().ok(),
                        },
                        committed_at: tool.timestamp.parse().unwrap_or_default(),
                    })
                    .map_err(persist_storage_error)?;
                Ok(())
            }
            _ => repository.update_manifest(|manifest| {
                manifest.current_leaf_id = entry.leaf_target_id().map(str::to_string);
                manifest.entries.push(entry.clone());
            }),
        }
    }

    pub fn apply_task_event(
        &self,
        session_dir: &Path,
        event: &TaskEvent,
    ) -> Result<(), SessionStorageError> {
        let repository = TaskRepository::new(session_dir);
        let (session_id, task_id, agent_id, parent_task_id, timestamp) =
            task_event_identity(&repository, event)?;
        if matches!(event, TaskEvent::Created { .. }) {
            repository
                .create_task(TaskShardHeader {
                    schema_version: SESSION_SCHEMA_VERSION,
                    session_id: session_id.clone(),
                    task_id: task_id.clone(),
                    agent_id: agent_id.clone(),
                    parent_task_id,
                    created_at: timestamp,
                })
                .map_err(persist_storage_error)?;
        }
        let task_seq = repository.next_task_seq(&session_id, &task_id)?;
        repository
            .commit_task_event(orchd_api::TaskEventCommit {
                session_id,
                task_id,
                agent_id,
                task_seq,
                event: event.clone(),
                committed_at: timestamp,
            })
            .map_err(persist_storage_error)?;
        Ok(())
    }

    pub fn append_session_info(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        name: &str,
        _agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry = SessionTreeEntry::SessionInfo(SessionInfoEntry {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            name: Some(name.to_string()),
        });
        let repository = TaskRepository::new(session_dir);
        repository.update_manifest(|manifest| {
            manifest.name = Some(name.to_string());
            manifest.entries.push(entry.clone());
        })?;
        Ok(entry)
    }

    pub fn append_config_metadata(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        model_id: Option<&str>,
        provider: Option<&str>,
        thinking_level: Option<&str>,
        _agent_id: Option<&str>,
    ) -> Result<Vec<SessionTreeEntry>, SessionStorageError> {
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
            entries.push(e);
        }
        if let Some(tl) = thinking_level {
            let e = SessionTreeEntry::ThinkingLevelChange(ThinkingLevelChangeEntry {
                id: Uuid::new_v4().to_string()[..8].to_string(),
                parent_id: cur,
                timestamp: timestamp(),
                thinking_level: tl.to_string(),
            });
            entries.push(e);
        }
        let repository = TaskRepository::new(session_dir);
        repository.update_manifest(|manifest| manifest.entries.extend(entries.clone()))?;
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
        self.append_entry(session_dir, &entry, agent_id)?;
        Ok(entry)
    }

    pub fn navigate(
        &self,
        session_dir: &Path,
        parent_id: Option<&str>,
        target_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<SessionTreeEntry, SessionStorageError> {
        let entry = SessionTreeEntry::Leaf(LeafEntry {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            parent_id: parent_id.map(str::to_string),
            timestamp: timestamp(),
            target_id: target_id.map(str::to_string),
        });
        self.append_entry(session_dir, &entry, agent_id)?;
        Ok(entry)
    }

    // ── fork / import ──

    pub fn fork(
        &self,
        _source_id: &str,
        source_dir: &Path,
        entry_id: Option<&str>,
    ) -> Result<PersistedSession, SessionStorageError> {
        if entry_id.is_some() {
            return Err(SessionStorageError::Invalid {
                path: source_dir.to_path_buf(),
                message: "branch-point fork is not yet supported by schema v2".into(),
            });
        }
        let source = TaskRepository::new(source_dir);
        let source_manifest = source.load_manifest()?;
        let forked_id = Uuid::new_v4().to_string();
        let created_at = timestamp();
        let cwd_dir = self.session_dir(&source_manifest.cwd);
        let forked_dir = cwd_dir.join(format!(
            "{}_{}",
            created_at.replace([':', '.'], "-"),
            forked_id
        ));
        source.fork_to(
            &forked_dir,
            forked_id,
            created_at.parse().unwrap_or_default(),
        )?;
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

#[allow(dead_code)]
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct TaskSidecar {
    #[serde(flatten)]
    tasks: BTreeMap<String, StoredTask>,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
            TaskEvent::Idle {
                task_id,
                agent_id,
                timestamp,
                ..
            } => {
                let task = self.entry(task_id);
                task.agent_id = Some(agent_id.clone());
                task.status = "idle".into();
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
            TaskEvent::Closed {
                task_id,
                agent_id,
                timestamp,
                ..
            } => {
                let task = self.entry(task_id);
                task.agent_id = Some(agent_id.clone());
                task.status = "closed".into();
                task.updated_at = Some(*timestamp);
            }
            TaskEvent::Reopened {
                task_id,
                agent_id,
                timestamp,
                ..
            } => {
                let task = self.entry(task_id);
                task.agent_id = Some(agent_id.clone());
                task.status = "idle".into();
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn task_status_from_sidecar(status: &str) -> AgentTaskStatus {
    match status {
        "queued" => AgentTaskStatus::Queued,
        "running" => AgentTaskStatus::Running,
        "idle" => AgentTaskStatus::Idle,
        "closed" => AgentTaskStatus::Closed,
        "completed" => AgentTaskStatus::Completed,
        "failed" => AgentTaskStatus::Failed,
        "cancelled" => AgentTaskStatus::Cancelled,
        _ => AgentTaskStatus::Failed,
    }
}

// ── helpers ──

fn encode_cwd(cwd: &str) -> String {
    format!(
        "cwd_{}",
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

fn task_event_identity(
    repository: &TaskRepository,
    event: &TaskEvent,
) -> Result<(String, String, String, Option<String>, i64), SessionStorageError> {
    match event {
        TaskEvent::Created {
            session_id,
            task_id,
            agent_id,
            parent_task_id,
            timestamp,
            ..
        } => Ok((
            session_id.clone(),
            task_id.clone(),
            agent_id.clone(),
            parent_task_id.clone(),
            *timestamp,
        )),
        TaskEvent::Started {
            session_id,
            task_id,
            agent_id,
            timestamp,
        }
        | TaskEvent::Idle {
            session_id,
            task_id,
            agent_id,
            timestamp,
            ..
        }
        | TaskEvent::Completed {
            session_id,
            task_id,
            agent_id,
            timestamp,
            ..
        }
        | TaskEvent::Failed {
            session_id,
            task_id,
            agent_id,
            timestamp,
            ..
        }
        | TaskEvent::Cancelled {
            session_id,
            task_id,
            agent_id,
            timestamp,
        }
        | TaskEvent::Closed {
            session_id,
            task_id,
            agent_id,
            timestamp,
        }
        | TaskEvent::Reopened {
            session_id,
            task_id,
            agent_id,
            timestamp,
        } => Ok((
            session_id.clone(),
            task_id.clone(),
            agent_id.clone(),
            None,
            *timestamp,
        )),
        TaskEvent::Joined {
            session_id,
            task_id,
            timestamp,
            ..
        }
        | TaskEvent::Steered {
            session_id,
            task_id,
            timestamp,
            ..
        } => {
            let manifest = repository.load_manifest()?;
            let agent_id = manifest
                .tasks
                .get(task_id)
                .map(|task| task.agent_id.clone())
                .ok_or_else(|| SessionStorageError::Invalid {
                    path: PathBuf::from("session.json"),
                    message: format!("task {task_id} missing from manifest"),
                })?;
            Ok((
                session_id.clone(),
                task_id.clone(),
                agent_id,
                None,
                *timestamp,
            ))
        }
    }
}

fn persist_storage_error(error: orchd_api::PersistError) -> SessionStorageError {
    SessionStorageError::Invalid {
        path: PathBuf::from("task shard"),
        message: error.to_string(),
    }
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
    let manifest_path = dir.join("session.json");
    if !manifest_path.exists() {
        return Err(SessionStorageError::Invalid {
            path: dir.to_path_buf(),
            message: "missing session.json".into(),
        });
    }
    let repository = TaskRepository::new(dir);
    let manifest = repository.load_manifest()?;
    let mut state = SessionState::new(manifest.session_id.clone(), manifest.cwd.clone());
    state.name = manifest.name.clone();
    state.active_task_id = manifest
        .active_task_id
        .clone()
        .or(manifest.root_task_id.clone());
    state.current_leaf_id = manifest.current_leaf_id.clone();
    state.entries = manifest.entries.clone();
    for task_id in repository.list_tasks(&manifest.session_id)? {
        let recovered = repository.load_task_for_recovery(&manifest.session_id, &task_id)?;
        let source = recovered
            .metadata
            .parent_task_id
            .as_ref()
            .map(|parent_task_id| TaskSource::Agent {
                agent_id: manifest
                    .tasks
                    .get(parent_task_id)
                    .map(|task| task.agent_id.clone())
                    .unwrap_or_else(|| "unknown".into()),
                task_id: parent_task_id.clone(),
            })
            .unwrap_or(TaskSource::User);
        state.tasks.insert(
            task_id.clone(),
            agent_task_state_from_recovered(&task_id, &recovered, source),
        );
        if let Some(lifecycle) = recovered.lifecycle.last() {
            state
                .task_lifecycle
                .insert(task_id.clone(), lifecycle.clone());
        }
        for work in &recovered.work_lifecycle {
            state
                .work_lifecycle
                .insert(work.work_id.clone(), work.clone());
        }
        for entry in transcript_entries_from_recovered(&recovered) {
            if let SessionTreeEntry::Message(message) = &entry {
                state.task_heads.insert(task_id.clone(), message.id.clone());
            }
            state.entries.push(entry);
        }
    }
    state.entries.sort_by_key(|e| e.timestamp().to_string());
    state.seq = state.entries.len() as u64;
    restore_agent_runtime_state(&mut state);
    Ok(PersistedSession {
        state,
        path: dir.to_path_buf(),
        created_at: manifest.created_at.to_string(),
        parent_session_path: None,
    })
}

#[allow(dead_code)]
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

fn restore_agent_runtime_state(state: &mut SessionState) {
    let specs = crate::domain::agents::loader::load_agents(&state.cwd);
    for task in state.tasks.values() {
        let spec = specs.get(&task.target_agent_id);
        state.active_agents.insert(
            task.id.clone(),
            AgentInfo {
                agent_id: task.target_agent_id.clone(),
                task_id: task.id.clone(),
                parent_task_id: task.parent_task_id.clone(),
                name: spec
                    .map(|spec| spec.name.clone())
                    .unwrap_or_else(|| task.target_agent_id.clone()),
                role: spec
                    .map(|spec| spec.role.clone())
                    .unwrap_or_else(|| "assistant".to_string()),
                status: agent_status_from_task_status(&task.status),
            },
        );
        state
            .agent_views
            .entry(task.id.clone())
            .or_insert_with(|| AgentViewState {
                task_id: task.id.clone(),
                agent_id: task.target_agent_id.clone(),
                events: VecDeque::new(),
                next_seq: 1,
            });
    }

    state.active_task_id = state
        .active_agents
        .values()
        .find(|agent| agent.parent_task_id.is_none())
        .map(|agent| agent.task_id.clone())
        .or_else(|| state.active_agents.keys().next().cloned());

    let entries = state.entries.clone();
    for entry in entries {
        for (task_id, agent_id, message) in project_agent_view_from_entry(&state.session_id, &entry)
        {
            let seq = state.next_agent_view_seq;
            state.next_agent_view_seq = state.next_agent_view_seq.saturating_add(1);
            let view = state
                .agent_views
                .entry(task_id.clone())
                .or_insert_with(|| AgentViewState {
                    task_id: task_id.clone(),
                    agent_id: agent_id.clone(),
                    events: VecDeque::new(),
                    next_seq: 1,
                });
            view.next_seq = seq.saturating_add(1);
            view.events
                .push_back(piko_protocol::SequencedServerMessage {
                    seq,
                    message: Box::new(message),
                });
        }
    }
}

fn project_agent_view_from_entry(
    session_id: &str,
    entry: &SessionTreeEntry,
) -> Vec<(String, String, ServerMessage)> {
    match entry {
        SessionTreeEntry::Message(message) => {
            let task_id = &message.task_id;
            let agent_id = &message.agent_id;
            match &message.message {
                Message::User { .. } | Message::Assistant { .. } | Message::ToolResult { .. } => {
                    vec![(
                        task_id.clone(),
                        agent_id.clone(),
                        ServerMessage::TranscriptCommitted(
                            piko_protocol::TranscriptCommittedEvent {
                                session_id: session_id.to_string(),
                                task_id: task_id.clone(),
                                agent_id: agent_id.clone(),
                                work_id: message.work_id.clone(),
                                message_id: message.id.clone(),
                                task_seq: message.task_seq,
                                message: message.message.clone(),
                            },
                        ),
                    )]
                }
                _ => Vec::new(),
            }
        }
        SessionTreeEntry::ToolCall(tool) => {
            let (Some(task_id), Some(agent_id)) = (&tool.task_id, &tool.agent_id) else {
                return Vec::new();
            };
            vec![(
                task_id.clone(),
                agent_id.clone(),
                ServerMessage::ToolExecution(piko_protocol::ToolExecutionEvent::Started {
                    task_id: task_id.clone(),
                    agent_id: agent_id.clone(),
                    tool_call_id: tool.tool_call_id.clone(),
                    tool_name: tool.tool_name.clone(),
                    args: tool.arguments.clone(),
                    parent_message_id: tool.parent_message_id.clone(),
                }),
            )]
        }
        _ => Vec::new(),
    }
}

fn agent_status_from_task_status(status: &AgentTaskStatus) -> AgentStatus {
    match status {
        AgentTaskStatus::Queued | AgentTaskStatus::Idle => AgentStatus::Idle,
        AgentTaskStatus::Running => AgentStatus::Running,
        AgentTaskStatus::Closed => AgentStatus::Closed,
        AgentTaskStatus::Completed => AgentStatus::Completed,
        AgentTaskStatus::Failed => AgentStatus::Failed,
        AgentTaskStatus::Cancelled => AgentStatus::Cancelled,
    }
}
