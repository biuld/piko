//! Schema-v3 agent-oriented session storage.
//!
//! Each durable `AgentInstance` owns exactly one append-only shard under
//! `agents/<agent_instance_id>.jsonl`. `session.json` is a rebuildable
//! manifest and never contains transcript messages.
//!
//! Unlike the legacy schema-v2 `TaskRepository`, there is no Task/Work
//! lifecycle projection and no per-execution shard: a single AgentInstance
//! shard accumulates its whole conversation across Turns and Executions.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use orchd_api::AgentCommitPort;
use piko_protocol::execution::{CommitAck, CommitError, MessageCommit};
use piko_protocol::{
    AgentCommitAck, AgentDurableCommand, AgentExecutionReport, AgentInboxItem,
    AgentInstanceIdentity, AgentInstanceLifecycle, Message, SessionTreeEntry,
};
use serde::{Deserialize, Serialize};

use super::SessionStorageError;

pub const SESSION_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionManifest {
    pub schema_version: u32,
    pub session_id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub current_leaf_id: Option<String>,
    #[serde(default)]
    pub root_agent_instance_id: Option<String>,
    #[serde(default)]
    pub agent_revision: u64,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentManifestEntry>,
    #[serde(default)]
    pub agent_inbox: Vec<AgentInboxItem>,
    #[serde(default)]
    pub agent_executions: BTreeMap<String, AgentExecutionManifestEntry>,
    /// Session-scoped metadata only; transcript messages never live here.
    pub entries: Vec<SessionTreeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentManifestEntry {
    pub identity: AgentInstanceIdentity,
    #[serde(default)]
    pub spec: Option<piko_protocol::AgentSpec>,
    pub lifecycle: AgentInstanceLifecycle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_report: Option<AgentExecutionReport>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionManifestEntry {
    pub agent_instance_id: String,
    pub execution_id: String,
    pub status: piko_protocol::ExecutionStatus,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<AgentExecutionReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentShardHeader {
    pub schema_version: u32,
    pub session_id: String,
    pub agent_instance_id: String,
    pub agent_spec_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommittedMessage {
    pub id: String,
    pub parent_id: Option<String>,
    pub agent_instance_id: String,
    pub agent_spec_id: String,
    #[serde(default)]
    pub execution_id: Option<String>,
    #[serde(default)]
    pub source_turn_id: Option<String>,
    pub transcript_seq: u64,
    pub timestamp: i64,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
enum AgentShardRecord {
    Header(AgentShardHeader),
    Message(CommittedMessage),
}

#[derive(Debug, Clone)]
pub struct RecoveredAgent {
    pub session_id: String,
    pub agent_instance_id: String,
    pub agent_spec_id: String,
    pub transcript: Vec<CommittedMessage>,
    pub head_message_id: Option<String>,
    pub last_transcript_seq: u64,
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    session_dir: PathBuf,
}

impl SessionStore {
    pub fn new(session_dir: impl Into<PathBuf>) -> Self {
        Self {
            session_dir: session_dir.into(),
        }
    }

    pub fn create_session(
        session_dir: impl Into<PathBuf>,
        session_id: String,
        cwd: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        let store = Self::new(session_dir);
        fs::create_dir_all(store.agents_dir()).map_err(|source| SessionStorageError::Io {
            path: store.agents_dir(),
            source,
        })?;
        let root_agent_instance_id = format!("agent_{session_id}_root");
        let root_identity = AgentInstanceIdentity {
            session_id: session_id.clone(),
            agent_instance_id: root_agent_instance_id.clone(),
            agent_spec_id: "main".into(),
            parent_agent_instance_id: None,
        };
        let mut agents = BTreeMap::new();
        agents.insert(
            root_agent_instance_id.clone(),
            AgentManifestEntry {
                identity: root_identity,
                spec: None,
                lifecycle: AgentInstanceLifecycle::Open,
                latest_report: None,
                created_at,
                updated_at: created_at,
            },
        );
        store.store_manifest(&SessionManifest {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id,
            cwd,
            name: None,
            created_at,
            updated_at: created_at,
            current_leaf_id: None,
            root_agent_instance_id: Some(root_agent_instance_id),
            agent_revision: 1,
            agents,
            agent_inbox: Vec::new(),
            agent_executions: BTreeMap::new(),
            entries: Vec::new(),
        })?;
        Ok(store)
    }

    /// Return the durable root AgentInstance, migrating a pre-AgentInstance
    /// manifest exactly once when necessary.
    pub fn ensure_root_agent(
        &self,
        agent_spec_id: &str,
    ) -> Result<AgentInstanceIdentity, SessionStorageError> {
        let mut manifest = self.load_manifest()?;
        if let Some(root_id) = &manifest.root_agent_instance_id
            && let Some(root) = manifest.agents.get(root_id)
        {
            return Ok(root.identity.clone());
        }

        let root_id = format!("agent_{}_root", manifest.session_id);
        let identity = AgentInstanceIdentity {
            session_id: manifest.session_id.clone(),
            agent_instance_id: root_id.clone(),
            agent_spec_id: agent_spec_id.to_string(),
            parent_agent_instance_id: None,
        };
        manifest.agent_revision = manifest.agent_revision.saturating_add(1);
        manifest.root_agent_instance_id = Some(root_id.clone());
        manifest.agents.insert(
            root_id,
            AgentManifestEntry {
                identity: identity.clone(),
                spec: None,
                lifecycle: AgentInstanceLifecycle::Open,
                latest_report: None,
                created_at: manifest.created_at,
                updated_at: manifest.updated_at,
            },
        );
        self.store_manifest(&manifest)?;
        Ok(identity)
    }

    pub fn agent_instances(&self) -> Result<Vec<AgentManifestEntry>, SessionStorageError> {
        Ok(self.load_manifest()?.agents.into_values().collect())
    }

    pub fn agent_inbox(
        &self,
        agent_instance_id: &str,
    ) -> Result<Vec<AgentInboxItem>, SessionStorageError> {
        Ok(self
            .load_manifest()?
            .agent_inbox
            .into_iter()
            .filter(|item| item.recipient_agent_instance_id == agent_instance_id)
            .collect())
    }

    pub fn agent_transcript(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<Vec<Message>, SessionStorageError> {
        let recovered = self.load_agent(session_id, agent_instance_id)?;
        Ok(recovered
            .transcript
            .into_iter()
            .map(|message| message.message)
            .collect())
    }

    pub fn agent_execution_reports(
        &self,
        agent_instance_id: &str,
    ) -> Result<Vec<AgentExecutionReport>, SessionStorageError> {
        Ok(self
            .load_manifest()?
            .agent_executions
            .into_values()
            .filter(|execution| execution.agent_instance_id == agent_instance_id)
            .filter_map(|execution| execution.report)
            .collect())
    }

    pub fn interrupt_incomplete_agent_executions(&self) -> Result<usize, SessionStorageError> {
        let mut manifest = self.load_manifest()?;
        let mut interrupted = 0;
        for execution in manifest.agent_executions.values_mut() {
            if !matches!(
                execution.status,
                piko_protocol::ExecutionStatus::Accepted | piko_protocol::ExecutionStatus::Running
            ) {
                continue;
            }
            let report = AgentExecutionReport {
                agent_instance_id: execution.agent_instance_id.clone(),
                execution_id: execution.execution_id.clone(),
                outcome: piko_protocol::ExecutionOutcome::Cancelled {
                    reason: Some("interrupted during session recovery".into()),
                },
                summary: "Execution interrupted during session recovery".into(),
                usage: Default::default(),
                artifacts: Vec::new(),
            };
            execution.status = piko_protocol::ExecutionStatus::Cancelled;
            execution.report = Some(report.clone());
            if let Some(agent) = manifest.agents.get_mut(&execution.agent_instance_id) {
                agent.latest_report = Some(report);
            }
            interrupted += 1;
        }
        if interrupted > 0 {
            manifest.agent_revision = manifest.agent_revision.saturating_add(interrupted as u64);
            manifest.updated_at = chrono::Utc::now().timestamp_millis();
            self.store_manifest(&manifest)?;
        }
        Ok(interrupted)
    }

    fn commit_agent_command_sync(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let mut manifest = self.load_manifest().map_err(storage_commit_error)?;
        if manifest.session_id != session_id {
            return Err(CommitError::IdentityMismatch);
        }

        let agent_instance_id = match command {
            AgentDurableCommand::Create { identity, spec } => {
                if identity.session_id != session_id {
                    return Err(CommitError::IdentityMismatch);
                }
                if let Some(existing) = manifest.agents.get_mut(&identity.agent_instance_id) {
                    if existing.identity == identity {
                        if existing.spec.is_none() {
                            existing.spec = Some(spec);
                            manifest.agent_revision = manifest.agent_revision.saturating_add(1);
                            let revision = manifest.agent_revision;
                            self.store_manifest(&manifest)
                                .map_err(storage_commit_error)?;
                            return Ok(AgentCommitAck {
                                session_id: session_id.to_string(),
                                agent_instance_id: identity.agent_instance_id,
                                revision,
                            });
                        }
                        if existing.spec.as_ref() != Some(&spec) {
                            return Err(CommitError::IdempotencyConflict);
                        }
                        return Ok(AgentCommitAck {
                            session_id: session_id.to_string(),
                            agent_instance_id: identity.agent_instance_id,
                            revision: manifest.agent_revision,
                        });
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                match &identity.parent_agent_instance_id {
                    Some(parent) if !manifest.agents.contains_key(parent) => {
                        return Err(CommitError::IdentityMismatch);
                    }
                    None if manifest.root_agent_instance_id.is_some() => {
                        return Err(CommitError::IdempotencyConflict);
                    }
                    None => {
                        manifest.root_agent_instance_id = Some(identity.agent_instance_id.clone())
                    }
                    Some(_) => {}
                }
                let id = identity.agent_instance_id.clone();
                let now = chrono::Utc::now().timestamp_millis();
                manifest.agents.insert(
                    id.clone(),
                    AgentManifestEntry {
                        identity,
                        spec: Some(spec),
                        lifecycle: AgentInstanceLifecycle::Open,
                        latest_report: None,
                        created_at: now,
                        updated_at: now,
                    },
                );
                id
            }
            AgentDurableCommand::SetLifecycle {
                agent_instance_id,
                lifecycle,
            } => {
                let agent = manifest
                    .agents
                    .get_mut(&agent_instance_id)
                    .ok_or(CommitError::IdentityMismatch)?;
                agent.lifecycle = lifecycle;
                agent.updated_at = chrono::Utc::now().timestamp_millis();
                agent_instance_id
            }
            AgentDurableCommand::ExecutionStarted {
                agent_instance_id,
                execution_id,
                started_at,
            } => {
                if !manifest.agents.contains_key(&agent_instance_id) {
                    return Err(CommitError::IdentityMismatch);
                }
                if let Some(existing) = manifest.agent_executions.get(&execution_id) {
                    if existing.agent_instance_id == agent_instance_id {
                        return Ok(AgentCommitAck {
                            session_id: session_id.to_string(),
                            agent_instance_id,
                            revision: manifest.agent_revision,
                        });
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                manifest.agent_executions.insert(
                    execution_id.clone(),
                    AgentExecutionManifestEntry {
                        agent_instance_id: agent_instance_id.clone(),
                        execution_id,
                        status: piko_protocol::ExecutionStatus::Accepted,
                        started_at,
                        report: None,
                    },
                );
                agent_instance_id
            }
            AgentDurableCommand::RecordExecutionReport { report } => {
                let source = manifest
                    .agents
                    .get_mut(&report.agent_instance_id)
                    .ok_or(CommitError::IdentityMismatch)?;
                source.latest_report = Some(report.clone());
                source.updated_at = chrono::Utc::now().timestamp_millis();
                if let Some(execution) = manifest.agent_executions.get_mut(&report.execution_id) {
                    if execution.agent_instance_id != report.agent_instance_id {
                        return Err(CommitError::IdentityMismatch);
                    }
                    execution.status = report.outcome.status();
                    execution.report = Some(report.clone());
                }
                report.agent_instance_id
            }
            AgentDurableCommand::CommitReport {
                recipient_agent_instance_id,
                report,
            } => {
                if !manifest.agents.contains_key(&recipient_agent_instance_id)
                    || !manifest.agents.contains_key(&report.agent_instance_id)
                {
                    return Err(CommitError::IdentityMismatch);
                }
                let report_id = format!(
                    "report_{}_{}",
                    report.agent_instance_id, report.execution_id
                );
                if !manifest
                    .agent_inbox
                    .iter()
                    .any(|item| item.report_id == report_id)
                {
                    manifest.agent_inbox.push(AgentInboxItem {
                        report_id,
                        recipient_agent_instance_id: recipient_agent_instance_id.clone(),
                        source_agent_instance_id: report.agent_instance_id.clone(),
                        report: report.clone(),
                        committed_at: chrono::Utc::now().timestamp_millis(),
                        consumed_at: None,
                    });
                }
                if let Some(source) = manifest.agents.get_mut(&report.agent_instance_id) {
                    source.latest_report = Some(report);
                }
                recipient_agent_instance_id
            }
            AgentDurableCommand::ConsumeInboxItem {
                agent_instance_id,
                report_id,
                consumed_at,
            } => {
                let item = manifest
                    .agent_inbox
                    .iter_mut()
                    .find(|item| {
                        item.report_id == report_id
                            && item.recipient_agent_instance_id == agent_instance_id
                    })
                    .ok_or(CommitError::IdentityMismatch)?;
                item.consumed_at = Some(consumed_at);
                agent_instance_id
            }
        };

        manifest.agent_revision = manifest.agent_revision.saturating_add(1);
        manifest.updated_at = chrono::Utc::now().timestamp_millis();
        let revision = manifest.agent_revision;
        self.store_manifest(&manifest)
            .map_err(storage_commit_error)?;
        Ok(AgentCommitAck {
            session_id: session_id.to_string(),
            agent_instance_id,
            revision,
        })
    }

    /// Ensure the durable shard for `agent_instance_id` exists (header-only).
    /// Idempotent: a matching existing header is a no-op.
    pub fn ensure_agent_shard(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        agent_spec_id: &str,
        created_at: i64,
    ) -> Result<(), SessionStorageError> {
        let path = self.agent_path(agent_instance_id);
        if path.exists() {
            let existing = self.read_header(&path)?;
            if existing.session_id == session_id && existing.agent_instance_id == agent_instance_id
            {
                return Ok(());
            }
            return Err(SessionStorageError::Invalid {
                path,
                message: "agent shard identity mismatch".into(),
            });
        }
        atomic_create_jsonl(
            &path,
            &AgentShardRecord::Header(AgentShardHeader {
                schema_version: SESSION_SCHEMA_VERSION,
                session_id: session_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                agent_spec_id: agent_spec_id.to_string(),
                created_at,
            }),
        )
    }

    /// Commit a Message onto the durable AgentInstance shard, auto-creating
    /// the shard header if this is the first write.
    pub fn commit_message(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
    ) -> Result<CommitAck, CommitError> {
        self.ensure_agent_shard(
            &commit.session_id,
            &commit.agent_instance_id,
            agent_spec_id,
            commit.committed_at,
        )
        .map_err(storage_commit_error)?;

        let recovered = self
            .load_agent(&commit.session_id, &commit.agent_instance_id)
            .map_err(storage_commit_error)?;

        if let Some(existing) = recovered
            .transcript
            .iter()
            .find(|message| message.id == commit.message_id)
        {
            if existing.parent_id == commit.parent_message_id
                && existing.message == commit.message
                && existing.execution_id.as_deref() == Some(commit.execution_id.as_str())
            {
                return Ok(CommitAck {
                    session_id: commit.session_id,
                    execution_id: commit.execution_id,
                    agent_instance_id: commit.agent_instance_id,
                    message_id: Some(commit.message_id),
                    revision: existing.transcript_seq,
                });
            }
            return Err(CommitError::IdempotencyConflict);
        }

        if commit.parent_message_id != recovered.head_message_id {
            return Err(CommitError::IdentityMismatch);
        }

        let transcript_seq = recovered.last_transcript_seq.saturating_add(1);
        let entry = CommittedMessage {
            id: commit.message_id.clone(),
            parent_id: commit.parent_message_id.clone(),
            agent_instance_id: commit.agent_instance_id.clone(),
            agent_spec_id: agent_spec_id.to_string(),
            execution_id: Some(commit.execution_id.clone()),
            source_turn_id: commit.source_turn_id.clone(),
            transcript_seq,
            timestamp: commit.committed_at,
            message: commit.message.clone(),
        };
        self.append_record(&commit.agent_instance_id, &AgentShardRecord::Message(entry))
            .map_err(storage_commit_error)?;

        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision: transcript_seq,
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

    pub fn update_manifest(
        &self,
        update: impl FnOnce(&mut SessionManifest),
    ) -> Result<(), SessionStorageError> {
        let mut manifest = self.load_manifest()?;
        update(&mut manifest);
        self.store_manifest(&manifest)
    }

    pub fn store_manifest(&self, manifest: &SessionManifest) -> Result<(), SessionStorageError> {
        atomic_write_json(&self.manifest_path(), manifest)
    }

    /// Load the full recovered transcript + head for one AgentInstance shard.
    pub fn load_agent(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<RecoveredAgent, SessionStorageError> {
        let path = self.agent_path(agent_instance_id);
        let records = read_records(&path)?;
        let Some(AgentShardRecord::Header(header)) = records.first().cloned() else {
            return Err(SessionStorageError::Invalid {
                path,
                message: "missing agent shard header".into(),
            });
        };
        if header.session_id != session_id || header.agent_instance_id != agent_instance_id {
            return Err(SessionStorageError::Invalid {
                path,
                message: "agent shard identity mismatch".into(),
            });
        }
        let mut transcript = Vec::new();
        let mut last_transcript_seq = 0;
        for record in records.into_iter().skip(1) {
            match record {
                AgentShardRecord::Message(message) => {
                    if message.agent_instance_id != agent_instance_id {
                        return Err(SessionStorageError::Invalid {
                            path: path.clone(),
                            message: "message identity mismatch".into(),
                        });
                    }
                    let seq = message.transcript_seq;
                    if seq != last_transcript_seq + 1 {
                        return Err(SessionStorageError::Invalid {
                            path: path.clone(),
                            message: format!(
                                "invalid transcript sequence: expected {}, got {seq}",
                                last_transcript_seq + 1
                            ),
                        });
                    }
                    last_transcript_seq = seq;
                    transcript.push(message);
                }
                AgentShardRecord::Header(_) => {
                    return Err(SessionStorageError::Invalid {
                        path: path.clone(),
                        message: "duplicate agent shard header".into(),
                    });
                }
            }
        }
        let head_message_id = transcript.last().map(|message| message.id.clone());
        Ok(RecoveredAgent {
            session_id: session_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
            agent_spec_id: header.agent_spec_id,
            transcript,
            head_message_id,
            last_transcript_seq,
        })
    }

    pub fn next_transcript_seq(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<u64, SessionStorageError> {
        Ok(self
            .load_agent(session_id, agent_instance_id)?
            .last_transcript_seq
            + 1)
    }

    pub fn find_committed_message(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Result<Option<CommittedMessage>, SessionStorageError> {
        let recovered = self.load_agent(session_id, agent_instance_id)?;
        Ok(recovered
            .transcript
            .into_iter()
            .find(|message| message.id == message_id))
    }

    /// Scan an agent shard for a message without requiring the full shard to
    /// parse. Used by the observation path when concurrent appends may leave
    /// a trailing partial line.
    pub fn find_committed_message_lenient(
        &self,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Option<CommittedMessage> {
        let path = self.agent_path(agent_instance_id);
        let file = File::open(&path).ok()?;
        for line in BufReader::new(file).lines() {
            let line = line.ok()?;
            if line.trim().is_empty() {
                continue;
            }
            let record = match serde_json::from_str::<AgentShardRecord>(&line) {
                Ok(record) => record,
                Err(_) => continue,
            };
            if let AgentShardRecord::Message(message) = record
                && message.id == message_id
                && message.agent_instance_id == agent_instance_id
            {
                return Some(message);
            }
        }
        None
    }

    /// List AgentInstance ids with a durable shard under `agents/`.
    pub fn list_agents(&self, session_id: &str) -> Result<Vec<String>, SessionStorageError> {
        let mut agents = Vec::new();
        if !self.agents_dir().exists() {
            return Ok(agents);
        }
        for entry in fs::read_dir(self.agents_dir()).map_err(|source| SessionStorageError::Io {
            path: self.agents_dir(),
            source,
        })? {
            let entry = entry.map_err(|source| SessionStorageError::Io {
                path: self.agents_dir(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
                let header = self.read_header(&path)?;
                if header.session_id != session_id {
                    return Err(SessionStorageError::Invalid {
                        path,
                        message: "agent shard belongs to another session".into(),
                    });
                }
                agents.push(header.agent_instance_id);
            }
        }
        agents.sort();
        Ok(agents)
    }

    pub fn fork_to(
        &self,
        destination: impl Into<PathBuf>,
        new_session_id: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        let source_manifest = self.load_manifest()?;
        let destination = Self::new(destination);
        fs::create_dir_all(destination.agents_dir()).map_err(|source| SessionStorageError::Io {
            path: destination.agents_dir(),
            source,
        })?;
        let mut manifest = source_manifest.clone();
        manifest.session_id = new_session_id.clone();
        manifest.created_at = created_at;
        manifest.updated_at = created_at;
        destination.store_manifest(&manifest)?;

        for agent_instance_id in self.list_agents(&source_manifest.session_id)? {
            let records = read_records(&self.agent_path(&agent_instance_id))?;
            let Some(AgentShardRecord::Header(mut header)) = records.first().cloned() else {
                return Err(SessionStorageError::Invalid {
                    path: self.agent_path(&agent_instance_id),
                    message: "missing agent shard header".into(),
                });
            };
            header.session_id = new_session_id.clone();
            atomic_create_jsonl(
                &destination.agent_path(&agent_instance_id),
                &AgentShardRecord::Header(header),
            )?;
            for record in records.into_iter().skip(1) {
                destination
                    .append_record(&agent_instance_id, &record)
                    .map_err(|error| SessionStorageError::Invalid {
                        path: destination.agent_path(&agent_instance_id),
                        message: error.to_string(),
                    })?;
            }
        }
        Ok(destination)
    }

    fn append_record(
        &self,
        agent_instance_id: &str,
        record: &AgentShardRecord,
    ) -> Result<(), SessionStorageError> {
        let path = self.agent_path(agent_instance_id);
        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|source| SessionStorageError::Io {
                path: path.clone(),
                source,
            })?;
        serde_json::to_writer(&mut file, record).map_err(|source| SessionStorageError::Json {
            path: path.clone(),
            source,
        })?;
        file.write_all(b"\n")
            .and_then(|_| file.sync_data())
            .map_err(|source| SessionStorageError::Io {
                path: path.clone(),
                source,
            })
    }

    fn read_header(&self, path: &Path) -> Result<AgentShardHeader, SessionStorageError> {
        match read_records(path)?.into_iter().next() {
            Some(AgentShardRecord::Header(header)) => Ok(header),
            _ => Err(SessionStorageError::Invalid {
                path: path.to_path_buf(),
                message: "missing agent shard header".into(),
            }),
        }
    }

    fn manifest_path(&self) -> PathBuf {
        self.session_dir.join("session.json")
    }

    fn agents_dir(&self) -> PathBuf {
        self.session_dir.join("agents")
    }

    fn agent_path(&self, agent_instance_id: &str) -> PathBuf {
        self.agents_dir().join(format!("{agent_instance_id}.jsonl"))
    }
}

#[async_trait]
impl AgentCommitPort for SessionStore {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        self.commit_agent_command_sync(session_id, command)
    }
}

fn storage_commit_error(error: SessionStorageError) -> CommitError {
    CommitError::Failed(error.to_string())
}

fn read_records(path: &Path) -> Result<Vec<AgentShardRecord>, SessionStorageError> {
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

fn atomic_create_jsonl(path: &Path, header: &AgentShardRecord) -> Result<(), SessionStorageError> {
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
