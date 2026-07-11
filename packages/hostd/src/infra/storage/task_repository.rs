//! Schema-v2 task-oriented session storage.
//!
//! Each task owns exactly one append-only shard. `session.json` is a
//! rebuildable manifest and never contains transcript messages.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use orchd_api::{
    MessageCommit, PersistAck, PersistError, PersistSink, TaskEventCommit, WorkEventCommit,
};
use piko_protocol::{AgentTaskStatus, Message, SessionTreeEntry, TaskEvent};
use serde::{Deserialize, Serialize};

use super::SessionStorageError;

pub const SESSION_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionManifest {
    pub schema_version: u32,
    pub session_id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub root_task_id: Option<String>,
    pub active_task_id: Option<String>,
    pub current_leaf_id: Option<String>,
    /// Session-scoped metadata only; transcript messages never live here.
    pub entries: Vec<SessionTreeEntry>,
    pub tasks: BTreeMap<String, TaskManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskManifestEntry {
    pub agent_id: String,
    pub parent_task_id: Option<String>,
    pub status: AgentTaskStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskShardHeader {
    pub schema_version: u32,
    pub session_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub parent_task_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommittedMessage {
    pub id: String,
    pub parent_id: Option<String>,
    pub task_id: String,
    pub agent_id: String,
    pub work_id: String,
    pub task_seq: u64,
    pub timestamp: i64,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TaskShardRecord {
    Header(TaskShardHeader),
    Message(CommittedMessage),
    Lifecycle {
        task_seq: u64,
        committed_at: i64,
        event: TaskEvent,
    },
    WorkLifecycle {
        task_seq: u64,
        committed_at: i64,
        snapshot: piko_protocol::agent_runtime::WorkSnapshot,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnLifecycleRecord {
    Submitted {
        session_id: String,
        turn_id: String,
        root_work_id: String,
        timestamp: i64,
    },
    Started {
        session_id: String,
        turn_id: String,
        root_task_id: String,
        root_work_id: String,
        timestamp: i64,
    },
    Completed {
        session_id: String,
        turn_id: String,
        root_task_id: String,
        root_work_id: String,
        total_tasks: u32,
        timestamp: i64,
    },
    Failed {
        session_id: String,
        turn_id: String,
        root_task_id: Option<String>,
        root_work_id: String,
        error: String,
        timestamp: i64,
    },
    Cancelled {
        session_id: String,
        turn_id: String,
        root_task_id: Option<String>,
        root_work_id: String,
        timestamp: i64,
    },
}

impl TurnLifecycleRecord {
    pub fn session_id(&self) -> &str {
        match self {
            Self::Submitted { session_id, .. }
            | Self::Started { session_id, .. }
            | Self::Completed { session_id, .. }
            | Self::Failed { session_id, .. }
            | Self::Cancelled { session_id, .. } => session_id,
        }
    }

    pub fn turn_id(&self) -> &str {
        match self {
            Self::Submitted { turn_id, .. }
            | Self::Started { turn_id, .. }
            | Self::Completed { turn_id, .. }
            | Self::Failed { turn_id, .. }
            | Self::Cancelled { turn_id, .. } => turn_id,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }

    pub fn root_work_id(&self) -> &str {
        match self {
            Self::Submitted { root_work_id, .. }
            | Self::Started { root_work_id, .. }
            | Self::Completed { root_work_id, .. }
            | Self::Failed { root_work_id, .. }
            | Self::Cancelled { root_work_id, .. } => root_work_id,
        }
    }

    pub fn root_task_id(&self) -> Option<&str> {
        match self {
            Self::Submitted { .. } => None,
            Self::Started { root_task_id, .. } | Self::Completed { root_task_id, .. } => {
                Some(root_task_id)
            }
            Self::Failed { root_task_id, .. } | Self::Cancelled { root_task_id, .. } => {
                root_task_id.as_deref()
            }
        }
    }

    fn same_terminal_outcome(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Completed {
                    total_tasks: left, ..
                },
                Self::Completed {
                    total_tasks: right, ..
                },
            ) => left == right,
            (Self::Failed { error: left, .. }, Self::Failed { error: right, .. }) => left == right,
            (Self::Cancelled { .. }, Self::Cancelled { .. }) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecoveredTask {
    pub metadata: TaskManifestEntry,
    pub transcript: Vec<CommittedMessage>,
    pub head_message_id: Option<String>,
    pub last_task_seq: u64,
    pub lifecycle: Vec<TaskEvent>,
    pub work_lifecycle: Vec<piko_protocol::agent_runtime::WorkSnapshot>,
}

#[derive(Debug, Clone)]
pub struct TaskRepository {
    session_dir: PathBuf,
}

impl TaskRepository {
    pub fn new(session_dir: impl Into<PathBuf>) -> Self {
        Self {
            session_dir: session_dir.into(),
        }
    }

    pub fn commit_turn_lifecycle(&self, record: TurnLifecycleRecord) -> Result<(), PersistError> {
        self.validate_manifest_identity(record.session_id())?;
        let existing = self.load_turn_lifecycle().map_err(storage_persist_error)?;
        if existing.iter().any(|item| item == &record) {
            return Ok(());
        }
        if record.is_terminal()
            && existing.iter().any(|item| {
                item.turn_id() == record.turn_id()
                    && item.root_work_id() == record.root_work_id()
                    && item.root_task_id() == record.root_task_id()
                    && item.same_terminal_outcome(&record)
            })
        {
            return Ok(());
        }
        if existing.iter().any(|item| {
            (item.turn_id() != record.turn_id() && item.root_work_id() == record.root_work_id())
                || (item.turn_id() == record.turn_id()
                    && (item.root_work_id() != record.root_work_id()
                        || matches!(
                            (item.root_task_id(), record.root_task_id()),
                            (Some(existing), Some(incoming)) if existing != incoming
                        )))
        }) {
            return Err(PersistError::IdentityMismatch);
        }
        if record.is_terminal()
            && existing.iter().any(|item| {
                item.turn_id() == record.turn_id() && item.is_terminal() && item != &record
            })
        {
            return Err(PersistError::IdempotencyConflict);
        }
        let path = self.session_dir.join("turns.jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| PersistError::Failed(format!("{}: {error}", path.display())))?;
        serde_json::to_writer(&mut file, &record)
            .map_err(|error| PersistError::Failed(error.to_string()))?;
        file.write_all(b"\n")
            .and_then(|_| file.sync_data())
            .map_err(|error| PersistError::Failed(error.to_string()))
    }

    pub fn load_turn_lifecycle(&self) -> Result<Vec<TurnLifecycleRecord>, SessionStorageError> {
        let path = self.session_dir.join("turns.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&path).map_err(|source| SessionStorageError::Io {
            path: path.clone(),
            source,
        })?;
        BufReader::new(file)
            .lines()
            .enumerate()
            .filter_map(|(index, line)| match line {
                Ok(line) if line.trim().is_empty() => None,
                other => Some((index, other)),
            })
            .map(|(index, line)| {
                let line = line.map_err(|source| SessionStorageError::Io {
                    path: path.clone(),
                    source,
                })?;
                serde_json::from_str(&line).map_err(|error| SessionStorageError::Invalid {
                    path: path.clone(),
                    message: format!("invalid turn record at line {}: {error}", index + 1),
                })
            })
            .collect()
    }

    pub fn load_turn_lifecycle_for_recovery(
        &self,
    ) -> Result<Vec<TurnLifecycleRecord>, SessionStorageError> {
        let path = self.session_dir.join("turns.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        repair_incomplete_jsonl_tail(&path)?;
        self.load_turn_lifecycle()
    }

    pub fn create_session(
        session_dir: impl Into<PathBuf>,
        session_id: String,
        cwd: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        let repository = Self::new(session_dir);
        fs::create_dir_all(repository.tasks_dir()).map_err(|source| SessionStorageError::Io {
            path: repository.tasks_dir(),
            source,
        })?;
        repository.store_manifest(&SessionManifest {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id,
            cwd,
            name: None,
            created_at,
            updated_at: created_at,
            root_task_id: None,
            active_task_id: None,
            current_leaf_id: None,
            entries: Vec::new(),
            tasks: BTreeMap::new(),
        })?;
        Ok(repository)
    }

    pub fn create_task(&self, header: TaskShardHeader) -> Result<PersistAck, PersistError> {
        self.validate_manifest_identity(&header.session_id)?;
        let path = self.task_path(&header.task_id);
        if path.exists() {
            let existing = self.read_header(&path).map_err(storage_persist_error)?;
            if existing == header {
                self.project_task_header(&header)?;
                return Ok(PersistAck {
                    session_id: header.session_id,
                    task_id: header.task_id,
                    message_id: None,
                    task_seq: 0,
                });
            }
            return Err(PersistError::IdempotencyConflict);
        }

        atomic_create_jsonl(&path, &TaskShardRecord::Header(header.clone()))
            .map_err(storage_persist_error)?;
        self.project_task_header(&header)?;
        Ok(PersistAck {
            session_id: header.session_id,
            task_id: header.task_id,
            message_id: None,
            task_seq: 0,
        })
    }

    fn project_task_header(&self, header: &TaskShardHeader) -> Result<(), PersistError> {
        let mut manifest = self.load_manifest().map_err(storage_persist_error)?;
        manifest
            .tasks
            .entry(header.task_id.clone())
            .or_insert_with(|| TaskManifestEntry {
                agent_id: header.agent_id.clone(),
                parent_task_id: header.parent_task_id.clone(),
                status: AgentTaskStatus::Queued,
                created_at: header.created_at,
                updated_at: header.created_at,
            });
        if header.parent_task_id.is_none() && manifest.root_task_id.is_none() {
            manifest.root_task_id = Some(header.task_id.clone());
        }
        manifest.updated_at = header.created_at;
        self.store_manifest(&manifest)
            .map_err(storage_persist_error)
    }

    pub fn commit_message(&self, commit: MessageCommit) -> Result<PersistAck, PersistError> {
        let recovered = self
            .load_task(&commit.session_id, &commit.task_id)
            .map_err(storage_persist_error)?;
        self.validate_agent(&recovered, &commit.agent_id)?;
        if let Some(existing) = recovered
            .transcript
            .iter()
            .find(|message| message.id == commit.message_id)
        {
            if existing.task_seq == commit.task_seq
                && existing.work_id == commit.work_id
                && existing.message == commit.message
            {
                return Ok(message_ack(&commit));
            }
            return Err(PersistError::IdempotencyConflict);
        }
        validate_next_sequence(recovered.last_task_seq, commit.task_seq)?;
        if commit.parent_message_id != recovered.head_message_id {
            return Err(PersistError::IdentityMismatch);
        }
        let entry = CommittedMessage {
            id: commit.message_id.clone(),
            parent_id: commit.parent_message_id.clone(),
            task_id: commit.task_id.clone(),
            agent_id: commit.agent_id.clone(),
            work_id: commit.work_id.clone(),
            task_seq: commit.task_seq,
            timestamp: commit.committed_at,
            message: commit.message.clone(),
        };
        self.append_record(&commit.task_id, &TaskShardRecord::Message(entry))?;
        Ok(message_ack(&commit))
    }

    pub fn commit_task_event(&self, commit: TaskEventCommit) -> Result<PersistAck, PersistError> {
        if let TaskEvent::Created {
            session_id,
            task_id,
            agent_id,
            parent_task_id,
            timestamp,
            ..
        } = &commit.event
        {
            if commit.task_seq != 1
                || session_id != &commit.session_id
                || task_id != &commit.task_id
                || agent_id != &commit.agent_id
            {
                return Err(PersistError::IdentityMismatch);
            }
            self.create_task(TaskShardHeader {
                schema_version: SESSION_SCHEMA_VERSION,
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                parent_task_id: parent_task_id.clone(),
                created_at: *timestamp,
            })?;
        }
        for record in
            read_records(&self.task_path(&commit.task_id)).map_err(storage_persist_error)?
        {
            match record {
                TaskShardRecord::Lifecycle {
                    task_seq, event, ..
                } if task_seq == commit.task_seq => {
                    if event == commit.event {
                        if !matches!(commit.event, TaskEvent::Created { .. }) {
                            self.project_lifecycle(&commit)?;
                        }
                        return Ok(PersistAck {
                            session_id: commit.session_id,
                            task_id: commit.task_id,
                            message_id: None,
                            task_seq: commit.task_seq,
                        });
                    }
                    return Err(PersistError::IdempotencyConflict);
                }
                TaskShardRecord::Message(message) if message.task_seq == commit.task_seq => {
                    return Err(PersistError::IdempotencyConflict);
                }
                TaskShardRecord::WorkLifecycle { task_seq, .. } if task_seq == commit.task_seq => {
                    return Err(PersistError::IdempotencyConflict);
                }
                _ => {}
            }
        }
        let recovered = self
            .load_task(&commit.session_id, &commit.task_id)
            .map_err(storage_persist_error)?;
        self.validate_agent(&recovered, &commit.agent_id)?;
        validate_next_sequence(recovered.last_task_seq, commit.task_seq)?;
        if commit.event.task_id() != commit.task_id {
            return Err(PersistError::IdentityMismatch);
        }
        self.append_record(
            &commit.task_id,
            &TaskShardRecord::Lifecycle {
                task_seq: commit.task_seq,
                committed_at: commit.committed_at,
                event: commit.event.clone(),
            },
        )?;
        self.project_lifecycle(&commit)?;
        Ok(PersistAck {
            session_id: commit.session_id,
            task_id: commit.task_id,
            message_id: None,
            task_seq: commit.task_seq,
        })
    }

    pub fn commit_work_event(&self, commit: WorkEventCommit) -> Result<PersistAck, PersistError> {
        for record in
            read_records(&self.task_path(&commit.task_id)).map_err(storage_persist_error)?
        {
            match record {
                TaskShardRecord::WorkLifecycle {
                    task_seq, snapshot, ..
                } if task_seq == commit.task_seq => {
                    if snapshot == commit.snapshot {
                        self.project_work_lifecycle(&commit)?;
                        return Ok(PersistAck {
                            session_id: commit.session_id,
                            task_id: commit.task_id,
                            message_id: None,
                            task_seq: commit.task_seq,
                        });
                    }
                    return Err(PersistError::IdempotencyConflict);
                }
                TaskShardRecord::Message(message) if message.task_seq == commit.task_seq => {
                    return Err(PersistError::IdempotencyConflict);
                }
                TaskShardRecord::Lifecycle { task_seq, .. } if task_seq == commit.task_seq => {
                    return Err(PersistError::IdempotencyConflict);
                }
                _ => {}
            }
        }
        let recovered = self
            .load_task(&commit.session_id, &commit.task_id)
            .map_err(storage_persist_error)?;
        self.validate_agent(&recovered, &commit.agent_id)?;
        validate_next_sequence(recovered.last_task_seq, commit.task_seq)?;
        self.append_record(
            &commit.task_id,
            &TaskShardRecord::WorkLifecycle {
                task_seq: commit.task_seq,
                committed_at: commit.committed_at,
                snapshot: commit.snapshot.clone(),
            },
        )?;
        self.project_work_lifecycle(&commit)?;
        Ok(PersistAck {
            session_id: commit.session_id,
            task_id: commit.task_id,
            message_id: None,
            task_seq: commit.task_seq,
        })
    }

    pub fn load_manifest(&self) -> Result<SessionManifest, SessionStorageError> {
        let path = self.manifest_path();
        let data = fs::read_to_string(&path).map_err(|source| SessionStorageError::Io {
            path: path.clone(),
            source,
        })?;
        let manifest: SessionManifest =
            serde_json::from_str(&data).map_err(|source| SessionStorageError::Json {
                path: path.clone(),
                source,
            })?;
        if manifest.schema_version != SESSION_SCHEMA_VERSION {
            return Err(SessionStorageError::Invalid {
                path,
                message: "unsupported session manifest schema".into(),
            });
        }
        Ok(manifest)
    }

    pub fn recover_session_tasks(
        &self,
        session_id: &str,
    ) -> Result<Vec<RecoveredTask>, SessionStorageError> {
        self.list_tasks(session_id)?
            .into_iter()
            .map(|task_id| self.load_task(session_id, &task_id))
            .collect()
    }

    pub fn load_task(
        &self,
        session_id: &str,
        task_id: &str,
    ) -> Result<RecoveredTask, SessionStorageError> {
        self.load_task_inner(session_id, task_id, false)
    }

    pub fn load_task_for_recovery(
        &self,
        session_id: &str,
        task_id: &str,
    ) -> Result<RecoveredTask, SessionStorageError> {
        self.load_task_inner(session_id, task_id, true)
    }

    fn load_task_inner(
        &self,
        session_id: &str,
        task_id: &str,
        allow_incomplete_tail: bool,
    ) -> Result<RecoveredTask, SessionStorageError> {
        self.validate_manifest_identity_storage(session_id)?;
        let path = self.task_path(task_id);
        let records = if allow_incomplete_tail {
            read_records_for_recovery(&path)?
        } else {
            read_records(&path)?
        };
        let Some(TaskShardRecord::Header(header)) = records.first().cloned() else {
            return Err(SessionStorageError::Invalid {
                path,
                message: "missing task shard header".into(),
            });
        };
        if header.session_id != session_id || header.task_id != task_id {
            return Err(SessionStorageError::Invalid {
                path,
                message: "task shard identity mismatch".into(),
            });
        }
        let mut transcript = Vec::new();
        let mut lifecycle = Vec::new();
        let mut work_lifecycle = Vec::new();
        let mut last_task_seq = 0;
        for record in records.into_iter().skip(1) {
            let seq = match record {
                TaskShardRecord::Message(message) => {
                    if message.session_identity(task_id, &header.agent_id).is_err() {
                        return Err(SessionStorageError::Invalid {
                            path: path.clone(),
                            message: "message identity mismatch".into(),
                        });
                    }
                    let seq = message.task_seq;
                    transcript.push(message);
                    seq
                }
                TaskShardRecord::Lifecycle {
                    task_seq, event, ..
                } => {
                    if event.task_id() != task_id {
                        return Err(SessionStorageError::Invalid {
                            path: path.clone(),
                            message: "lifecycle identity mismatch".into(),
                        });
                    }
                    lifecycle.push(event);
                    task_seq
                }
                TaskShardRecord::WorkLifecycle {
                    task_seq, snapshot, ..
                } => {
                    work_lifecycle.push(snapshot);
                    task_seq
                }
                TaskShardRecord::Header(_) => {
                    return Err(SessionStorageError::Invalid {
                        path: path.clone(),
                        message: "duplicate task shard header".into(),
                    });
                }
            };
            if seq != last_task_seq + 1 {
                return Err(SessionStorageError::Invalid {
                    path: path.clone(),
                    message: format!(
                        "invalid task sequence: expected {}, got {seq}",
                        last_task_seq + 1
                    ),
                });
            }
            last_task_seq = seq;
        }
        let manifest = self.load_manifest()?;
        let metadata = manifest
            .tasks
            .get(task_id)
            .cloned()
            .unwrap_or_else(|| TaskManifestEntry {
                agent_id: header.agent_id.clone(),
                parent_task_id: header.parent_task_id.clone(),
                status: lifecycle
                    .last()
                    .and_then(task_status_from_lifecycle)
                    .unwrap_or(AgentTaskStatus::Idle),
                created_at: header.created_at,
                updated_at: lifecycle
                    .last()
                    .map(lifecycle_event_timestamp)
                    .unwrap_or(header.created_at),
            });
        let head_message_id = transcript.last().map(|message| message.id.clone());
        Ok(RecoveredTask {
            metadata,
            transcript,
            head_message_id,
            last_task_seq,
            lifecycle,
            work_lifecycle,
        })
    }

    pub fn next_task_seq(
        &self,
        session_id: &str,
        task_id: &str,
    ) -> Result<u64, SessionStorageError> {
        Ok(self.load_task(session_id, task_id)?.last_task_seq + 1)
    }

    pub fn find_committed_message(
        &self,
        session_id: &str,
        task_id: &str,
        message_id: &str,
    ) -> Result<Option<CommittedMessage>, SessionStorageError> {
        let recovered = self.load_task(session_id, task_id)?;
        self.validate_manifest_identity_storage(session_id)?;
        Ok(recovered
            .transcript
            .into_iter()
            .find(|message| message.id == message_id))
    }

    /// Scan a task shard for a message without requiring the full shard to parse.
    ///
    /// Used by the observation path when lifecycle records are being appended
    /// concurrently and strict `load_task` validation may fail on a trailing line.
    /// Recovery/diagnostic-only scan that tolerates an incomplete crash tail.
    /// Live observation must use HostState instead.
    pub fn find_committed_message_for_recovery_lenient(
        &self,
        task_id: &str,
        message_id: &str,
    ) -> Option<CommittedMessage> {
        let path = self.task_path(task_id);
        let file = File::open(&path).ok()?;
        for line in BufReader::new(file).lines() {
            let line = line.ok()?;
            if line.trim().is_empty() {
                continue;
            }
            let record = match serde_json::from_str::<TaskShardRecord>(&line) {
                Ok(record) => record,
                Err(_) => continue,
            };
            if let TaskShardRecord::Message(message) = record
                && message.id == message_id
                && message.task_id == task_id
            {
                return Some(message);
            }
        }
        None
    }

    pub fn update_manifest(
        &self,
        update: impl FnOnce(&mut SessionManifest),
    ) -> Result<(), SessionStorageError> {
        let mut manifest = self.load_manifest()?;
        update(&mut manifest);
        self.store_manifest(&manifest)
    }

    pub fn list_tasks(&self, session_id: &str) -> Result<Vec<String>, SessionStorageError> {
        self.validate_manifest_identity_storage(session_id)?;
        let mut tasks = Vec::new();
        for entry in fs::read_dir(self.tasks_dir()).map_err(|source| SessionStorageError::Io {
            path: self.tasks_dir(),
            source,
        })? {
            let entry = entry.map_err(|source| SessionStorageError::Io {
                path: self.tasks_dir(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
                let header = self.read_header(&path)?;
                if header.session_id != session_id {
                    return Err(SessionStorageError::Invalid {
                        path,
                        message: "task shard belongs to another session".into(),
                    });
                }
                tasks.push(header.task_id);
            }
        }
        tasks.sort();
        Ok(tasks)
    }

    pub fn fork_to(
        &self,
        destination: impl Into<PathBuf>,
        new_session_id: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        let source_manifest = self.load_manifest()?;
        let destination = Self::new(destination);
        fs::create_dir_all(destination.tasks_dir()).map_err(|source| SessionStorageError::Io {
            path: destination.tasks_dir(),
            source,
        })?;
        let mut manifest = source_manifest.clone();
        manifest.session_id = new_session_id.clone();
        manifest.created_at = created_at;
        manifest.updated_at = created_at;
        destination.store_manifest(&manifest)?;

        for task_id in self.list_tasks(&source_manifest.session_id)? {
            let records = read_records(&self.task_path(&task_id))?;
            let Some(TaskShardRecord::Header(mut header)) = records.first().cloned() else {
                return Err(SessionStorageError::Invalid {
                    path: self.task_path(&task_id),
                    message: "missing task shard header".into(),
                });
            };
            header.session_id = new_session_id.clone();
            atomic_create_jsonl(
                &destination.task_path(&task_id),
                &TaskShardRecord::Header(header),
            )?;
            for record in records.into_iter().skip(1) {
                let record = match record {
                    TaskShardRecord::Lifecycle {
                        task_seq,
                        committed_at,
                        event,
                    } => TaskShardRecord::Lifecycle {
                        task_seq,
                        committed_at,
                        event: rewrite_task_event_session(event, &new_session_id)?,
                    },
                    other => other,
                };
                destination
                    .append_record(&task_id, &record)
                    .map_err(|error| SessionStorageError::Invalid {
                        path: destination.task_path(&task_id),
                        message: error.to_string(),
                    })?;
            }
        }
        Ok(destination)
    }

    fn append_record(&self, task_id: &str, record: &TaskShardRecord) -> Result<(), PersistError> {
        let path = self.task_path(task_id);
        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|error| PersistError::Failed(format!("{}: {error}", path.display())))?;
        serde_json::to_writer(&mut file, record)
            .map_err(|error| PersistError::Failed(error.to_string()))?;
        file.write_all(b"\n")
            .and_then(|_| file.sync_data())
            .map_err(|error| PersistError::Failed(error.to_string()))
    }

    fn project_lifecycle(&self, commit: &TaskEventCommit) -> Result<(), PersistError> {
        let mut manifest = self.load_manifest().map_err(storage_persist_error)?;
        let task = manifest
            .tasks
            .get_mut(&commit.task_id)
            .ok_or(PersistError::IdentityMismatch)?;
        if task.updated_at > commit.committed_at {
            return Ok(());
        }
        task.status = lifecycle_status(&commit.event, &task.status);
        task.updated_at = commit.committed_at;
        manifest.updated_at = commit.committed_at;
        self.store_manifest(&manifest)
            .map_err(storage_persist_error)
    }

    fn project_work_lifecycle(&self, commit: &WorkEventCommit) -> Result<(), PersistError> {
        if !matches!(
            commit.snapshot.status,
            piko_protocol::agent_runtime::WorkStatus::Cancelled
        ) {
            return Ok(());
        }
        self.update_manifest(|manifest| {
            if let Some(task) = manifest.tasks.get_mut(&commit.task_id) {
                if task.updated_at > commit.committed_at {
                    return;
                }
                task.status = AgentTaskStatus::Idle;
                task.updated_at = commit.committed_at;
            }
            manifest.updated_at = manifest.updated_at.max(commit.committed_at);
        })
        .map_err(storage_persist_error)
    }

    fn validate_agent(&self, task: &RecoveredTask, agent_id: &str) -> Result<(), PersistError> {
        if task.metadata.agent_id == agent_id {
            Ok(())
        } else {
            Err(PersistError::IdentityMismatch)
        }
    }

    fn validate_manifest_identity(&self, session_id: &str) -> Result<(), PersistError> {
        self.validate_manifest_identity_storage(session_id)
            .map_err(storage_persist_error)
    }

    fn validate_manifest_identity_storage(
        &self,
        session_id: &str,
    ) -> Result<(), SessionStorageError> {
        let manifest = self.load_manifest()?;
        if manifest.session_id == session_id {
            Ok(())
        } else {
            Err(SessionStorageError::Invalid {
                path: self.manifest_path(),
                message: "session identity mismatch".into(),
            })
        }
    }

    fn read_header(&self, path: &Path) -> Result<TaskShardHeader, SessionStorageError> {
        match read_records(path)?.into_iter().next() {
            Some(TaskShardRecord::Header(header)) => Ok(header),
            _ => Err(SessionStorageError::Invalid {
                path: path.to_path_buf(),
                message: "missing task shard header".into(),
            }),
        }
    }

    fn store_manifest(&self, manifest: &SessionManifest) -> Result<(), SessionStorageError> {
        atomic_write_json(&self.manifest_path(), manifest)
    }

    fn manifest_path(&self) -> PathBuf {
        self.session_dir.join("session.json")
    }

    fn tasks_dir(&self) -> PathBuf {
        self.session_dir.join("tasks")
    }

    fn task_path(&self, task_id: &str) -> PathBuf {
        self.tasks_dir().join(format!("{task_id}.jsonl"))
    }
}

#[async_trait]
impl PersistSink for TaskRepository {
    async fn commit_message(&self, event: MessageCommit) -> Result<PersistAck, PersistError> {
        TaskRepository::commit_message(self, event)
    }

    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError> {
        TaskRepository::commit_task_event(self, event)
    }

    async fn commit_work_event(&self, event: WorkEventCommit) -> Result<PersistAck, PersistError> {
        TaskRepository::commit_work_event(self, event)
    }
}

impl CommittedMessage {
    fn session_identity(&self, task_id: &str, agent_id: &str) -> Result<(), ()> {
        if self.task_id == task_id && self.agent_id == agent_id {
            Ok(())
        } else {
            Err(())
        }
    }
}

fn validate_next_sequence(last: u64, actual: u64) -> Result<(), PersistError> {
    let expected = last + 1;
    if actual == expected {
        Ok(())
    } else {
        Err(PersistError::SequenceMismatch { expected, actual })
    }
}

fn message_ack(commit: &MessageCommit) -> PersistAck {
    PersistAck {
        session_id: commit.session_id.clone(),
        task_id: commit.task_id.clone(),
        message_id: Some(commit.message_id.clone()),
        task_seq: commit.task_seq,
    }
}

fn lifecycle_status(event: &TaskEvent, previous: &AgentTaskStatus) -> AgentTaskStatus {
    match event {
        TaskEvent::Created { .. } => AgentTaskStatus::Queued,
        TaskEvent::Started { .. } | TaskEvent::Steered { .. } => AgentTaskStatus::Running,
        TaskEvent::Idle { .. } | TaskEvent::Reopened { .. } => AgentTaskStatus::Idle,
        TaskEvent::Completed { .. } => AgentTaskStatus::Completed,
        TaskEvent::Failed { .. } => AgentTaskStatus::Failed,
        TaskEvent::Cancelled { .. } => AgentTaskStatus::Cancelled,
        TaskEvent::Closed { .. } => AgentTaskStatus::Closed,
        TaskEvent::Joined { .. } => previous.clone(),
    }
}

fn read_records(path: &Path) -> Result<Vec<TaskShardRecord>, SessionStorageError> {
    let file = File::open(path).map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(index, line)| match line {
            Ok(line) if line.trim().is_empty() => None,
            value => Some((index, value)),
        })
        .map(|(index, line)| {
            let line = line.map_err(|source| SessionStorageError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            serde_json::from_str(&line).map_err(|source| SessionStorageError::Invalid {
                path: path.to_path_buf(),
                message: format!("invalid record at line {}: {source}", index + 1),
            })
        })
        .collect()
}

fn read_records_for_recovery(path: &Path) -> Result<Vec<TaskShardRecord>, SessionStorageError> {
    repair_incomplete_jsonl_tail(path)?;
    read_records(path)
}

fn repair_incomplete_jsonl_tail(path: &Path) -> Result<(), SessionStorageError> {
    let bytes = fs::read(path).map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if bytes.is_empty() || bytes.ends_with(b"\n") {
        return Ok(());
    }
    let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') else {
        return Err(SessionStorageError::Invalid {
            path: path.to_path_buf(),
            message: "incomplete JSONL header".into(),
        });
    };
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|source| SessionStorageError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.set_len((last_newline + 1) as u64)
        .and_then(|_| file.sync_data())
        .map_err(|source| SessionStorageError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn atomic_create_jsonl(path: &Path, header: &TaskShardRecord) -> Result<(), SessionStorageError> {
    let tmp = path.with_extension("jsonl.tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|source| SessionStorageError::Io {
            path: tmp.clone(),
            source,
        })?;
    serde_json::to_writer(&mut file, header).map_err(|source| SessionStorageError::Json {
        path: tmp.clone(),
        source,
    })?;
    file.write_all(b"\n")
        .and_then(|_| file.sync_all())
        .map_err(|source| SessionStorageError::Io {
            path: tmp.clone(),
            source,
        })?;
    fs::rename(&tmp, path).map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn atomic_write_json(path: &Path, value: &impl Serialize) -> Result<(), SessionStorageError> {
    let tmp = path.with_extension("json.tmp");
    let mut file = File::create(&tmp).map_err(|source| SessionStorageError::Io {
        path: tmp.clone(),
        source,
    })?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|source| SessionStorageError::Json {
        path: tmp.clone(),
        source,
    })?;
    file.write_all(b"\n")
        .and_then(|_| file.sync_all())
        .map_err(|source| SessionStorageError::Io {
            path: tmp.clone(),
            source,
        })?;
    fs::rename(&tmp, path).map_err(|source| SessionStorageError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn storage_persist_error(error: SessionStorageError) -> PersistError {
    PersistError::Failed(error.to_string())
}

fn lifecycle_event_timestamp(event: &TaskEvent) -> i64 {
    match event {
        TaskEvent::Created { timestamp, .. }
        | TaskEvent::Started { timestamp, .. }
        | TaskEvent::Idle { timestamp, .. }
        | TaskEvent::Completed { timestamp, .. }
        | TaskEvent::Failed { timestamp, .. }
        | TaskEvent::Cancelled { timestamp, .. }
        | TaskEvent::Closed { timestamp, .. }
        | TaskEvent::Reopened { timestamp, .. }
        | TaskEvent::Joined { timestamp, .. }
        | TaskEvent::Steered { timestamp, .. } => *timestamp,
    }
}

fn task_status_from_lifecycle(event: &TaskEvent) -> Option<AgentTaskStatus> {
    match event {
        TaskEvent::Idle { .. } => Some(AgentTaskStatus::Idle),
        TaskEvent::Started { .. } => Some(AgentTaskStatus::Running),
        TaskEvent::Completed { .. } => Some(AgentTaskStatus::Completed),
        TaskEvent::Failed { .. } => Some(AgentTaskStatus::Failed),
        TaskEvent::Cancelled { .. } => Some(AgentTaskStatus::Cancelled),
        TaskEvent::Closed { .. } => Some(AgentTaskStatus::Closed),
        TaskEvent::Reopened { .. } => Some(AgentTaskStatus::Idle),
        TaskEvent::Created { .. } => Some(AgentTaskStatus::Queued),
        TaskEvent::Joined { .. } | TaskEvent::Steered { .. } => None,
    }
}

fn rewrite_task_event_session(
    event: TaskEvent,
    session_id: &str,
) -> Result<TaskEvent, SessionStorageError> {
    let mut value = serde_json::to_value(event).map_err(|source| SessionStorageError::Json {
        path: PathBuf::from("task lifecycle"),
        source,
    })?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| SessionStorageError::Invalid {
            path: PathBuf::from("task lifecycle"),
            message: "task event is not an object".into(),
        })?;
    object.insert(
        "session_id".into(),
        serde_json::Value::String(session_id.to_string()),
    );
    serde_json::from_value(value).map_err(|source| SessionStorageError::Json {
        path: PathBuf::from("task lifecycle"),
        source,
    })
}

#[cfg(test)]
mod tests {
    use piko_protocol::MessageContent;
    use tempfile::tempdir;

    use super::*;

    fn message_commit(seq: u64, id: &str, parent: Option<&str>) -> MessageCommit {
        MessageCommit {
            session_id: "session-1".into(),
            task_id: "task-1".into(),
            agent_id: "coder".into(),
            work_id: "work-1".into(),
            task_seq: seq,
            message_id: id.into(),
            parent_message_id: parent.map(str::to_string),
            message: Message::User {
                content: MessageContent::String("hello".into()),
                timestamp: Some(2),
            },
            committed_at: 2,
        }
    }

    #[test]
    fn task_created_commit_creates_authoritative_shard() {
        let temp = tempdir().unwrap();
        let repository =
            TaskRepository::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
                .unwrap();
        let created = TaskEventCommit {
            session_id: "session-1".into(),
            task_id: "task-1".into(),
            agent_id: "coder".into(),
            task_seq: 1,
            event: TaskEvent::Created {
                session_id: "session-1".into(),
                work_id: "work-bootstrap".into(),
                task_id: "task-1".into(),
                agent_id: "coder".into(),
                parent_task_id: None,
                source_agent_id: None,
                prompt: String::new(),
                timestamp: 1,
            },
            committed_at: 1,
        };

        repository.commit_task_event(created.clone()).unwrap();
        repository.commit_task_event(created).unwrap();
        let recovered = repository.load_task("session-1", "task-1").unwrap();
        assert_eq!(recovered.last_task_seq, 1);
        assert_eq!(recovered.lifecycle.len(), 1);
    }

    #[test]
    fn stores_and_recovers_independent_task_shards() {
        let temp = tempdir().unwrap();
        let repository =
            TaskRepository::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
                .unwrap();
        repository
            .create_task(TaskShardHeader {
                schema_version: SESSION_SCHEMA_VERSION,
                session_id: "session-1".into(),
                task_id: "task-1".into(),
                agent_id: "coder".into(),
                parent_task_id: None,
                created_at: 1,
            })
            .unwrap();
        repository
            .commit_message(message_commit(1, "message-1", None))
            .unwrap();
        repository
            .commit_message(message_commit(2, "message-2", Some("message-1")))
            .unwrap();
        let work_commit = WorkEventCommit {
            session_id: "session-1".into(),
            task_id: "task-1".into(),
            agent_id: "coder".into(),
            task_seq: 3,
            snapshot: piko_protocol::agent_runtime::WorkSnapshot {
                work_id: "work-1".into(),
                status: piko_protocol::agent_runtime::WorkStatus::Cancelled,
                source_turn_id: Some("turn-1".into()),
            },
            committed_at: 3,
        };
        repository.commit_work_event(work_commit.clone()).unwrap();
        repository.commit_work_event(work_commit).unwrap();
        let task_commit = TaskEventCommit {
            session_id: "session-1".into(),
            task_id: "task-1".into(),
            agent_id: "coder".into(),
            task_seq: 4,
            event: TaskEvent::Idle {
                session_id: "session-1".into(),
                task_id: "task-1".into(),
                agent_id: "coder".into(),
                total_steps: 1,
                summary: "done".into(),
                timestamp: 4,
            },
            committed_at: 4,
        };
        repository.commit_task_event(task_commit.clone()).unwrap();
        repository.commit_task_event(task_commit).unwrap();

        let recovered = repository.load_task("session-1", "task-1").unwrap();
        assert_eq!(recovered.transcript.len(), 2);
        assert_eq!(recovered.head_message_id.as_deref(), Some("message-2"));
        assert_eq!(recovered.last_task_seq, 4);
        assert_eq!(recovered.work_lifecycle.len(), 1);
        assert_eq!(recovered.lifecycle.len(), 1);
        assert!(temp.path().join("tasks/task-1.jsonl").exists());
        assert!(!temp.path().join("main.jsonl").exists());
        assert!(!temp.path().join("tasks.json").exists());
    }

    #[test]
    fn rejects_cross_task_parent_and_duplicate_payload_conflict() {
        let temp = tempdir().unwrap();
        let repository =
            TaskRepository::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
                .unwrap();
        repository
            .create_task(TaskShardHeader {
                schema_version: SESSION_SCHEMA_VERSION,
                session_id: "session-1".into(),
                task_id: "task-1".into(),
                agent_id: "coder".into(),
                parent_task_id: None,
                created_at: 1,
            })
            .unwrap();
        repository
            .commit_message(message_commit(1, "message-1", None))
            .unwrap();
        let wrong_parent = message_commit(2, "message-2", Some("other-task-message"));
        assert_eq!(
            repository.commit_message(wrong_parent),
            Err(PersistError::IdentityMismatch)
        );

        let mut conflict = message_commit(1, "message-1", None);
        conflict.work_id = "different-work".into();
        assert_eq!(
            repository.commit_message(conflict),
            Err(PersistError::IdempotencyConflict)
        );
    }

    #[test]
    fn recovery_message_lookup_tolerates_trailing_partial_line() {
        use std::io::Write;

        let temp = tempdir().unwrap();
        let repository =
            TaskRepository::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
                .unwrap();
        repository
            .commit_task_event(TaskEventCommit {
                session_id: "session-1".into(),
                task_id: "task-1".into(),
                agent_id: "main".into(),
                task_seq: 1,
                event: TaskEvent::Created {
                    session_id: "session-1".into(),
                    work_id: "work-1".into(),
                    task_id: "task-1".into(),
                    agent_id: "main".into(),
                    parent_task_id: None,
                    source_agent_id: None,
                    prompt: String::new(),
                    timestamp: 1,
                },
                committed_at: 1,
            })
            .unwrap();
        repository
            .commit_message(MessageCommit {
                session_id: "session-1".into(),
                task_id: "task-1".into(),
                agent_id: "main".into(),
                work_id: "work-2".into(),
                task_seq: 2,
                message_id: "msg-followup".into(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String("second turn".into()),
                    timestamp: Some(2),
                },
                committed_at: 2,
            })
            .unwrap();

        let shard = repository.task_path("task-1");
        let mut file = OpenOptions::new().append(true).open(&shard).unwrap();
        write!(file, "{{\"type\":\"lifecycle\",\"task_seq\":3").unwrap();

        assert!(repository.load_task("session-1", "task-1").is_err());
        let found = repository
            .find_committed_message_for_recovery_lenient("task-1", "msg-followup")
            .expect("lenient scan should still find committed message");
        assert_eq!(found.id, "msg-followup");
    }

    #[test]
    fn turn_lifecycle_terminal_is_durable_and_idempotent() {
        let temp = tempdir().unwrap();
        let repository =
            TaskRepository::create_session(temp.path(), "session-1".into(), "/project".into(), 1)
                .unwrap();
        repository
            .commit_turn_lifecycle(TurnLifecycleRecord::Submitted {
                session_id: "session-1".into(),
                turn_id: "turn-1".into(),
                root_work_id: "work-1".into(),
                timestamp: 1,
            })
            .unwrap();
        let completed = TurnLifecycleRecord::Completed {
            session_id: "session-1".into(),
            turn_id: "turn-1".into(),
            root_task_id: "task-1".into(),
            root_work_id: "work-1".into(),
            total_tasks: 1,
            timestamp: 2,
        };
        repository.commit_turn_lifecycle(completed.clone()).unwrap();
        repository.commit_turn_lifecycle(completed).unwrap();
        assert_eq!(repository.load_turn_lifecycle().unwrap().len(), 2);

        assert!(matches!(
            repository.commit_turn_lifecycle(TurnLifecycleRecord::Failed {
                session_id: "session-1".into(),
                turn_id: "turn-1".into(),
                root_task_id: Some("task-1".into()),
                root_work_id: "work-1".into(),
                error: "late failure".into(),
                timestamp: 3,
            }),
            Err(PersistError::IdempotencyConflict)
        ));
    }
}
