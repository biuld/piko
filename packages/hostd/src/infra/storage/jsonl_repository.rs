use std::collections::VecDeque;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use crate::api::{
    AgentInfo, AgentStatus, Message, ServerMessage, SessionInfoEntry, SessionSummary,
    SessionTreeEntry, ThinkingLevelChangeEntry,
};
use uuid::Uuid;

use super::jsonl_io::SessionHeader;
use super::recovery::{agent_task_state_from_manifest_entry, agent_transcript_entries};
use super::session_store::{SessionManifest, SessionStore};
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
        SessionStore::create_session(
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
        let store = SessionStore::new(session_dir);
        match entry {
            SessionTreeEntry::Message(message) => {
                let agent_instance_id = message.agent_instance_id.clone();
                let manifest = store.load_manifest()?;
                let agent_spec_id = manifest
                    .agents
                    .get(&agent_instance_id)
                    .map(|agent| agent.identity.agent_spec_id.clone())
                    .unwrap_or_else(|| message.agent_id.clone());
                store
                    .commit_message(
                        piko_protocol::execution::MessageCommit {
                            session_id: manifest.session_id,
                            source_turn_id: Some(message.source_turn_id.clone()),
                            execution_id: message.source_turn_id.clone(),
                            agent_instance_id,
                            message_id: message.id.clone(),
                            parent_message_id: message.parent_id.clone(),
                            message: message.message.clone(),
                            committed_at: message.timestamp.parse().unwrap_or_default(),
                        },
                        &agent_spec_id,
                    )
                    .map_err(commit_storage_error)?;
                Ok(())
            }
            SessionTreeEntry::ToolCall(tool) => {
                let (Some(agent_instance_id), Some(agent_spec_id)) =
                    (&tool.task_id, &tool.agent_id)
                else {
                    return Err(SessionStorageError::Invalid {
                        path: session_dir.to_path_buf(),
                        message: "tool entry requires task_id and agent_id".into(),
                    });
                };
                let manifest = store.load_manifest()?;
                store
                    .commit_message(
                        piko_protocol::execution::MessageCommit {
                            session_id: manifest.session_id,
                            source_turn_id: Some(agent_instance_id.clone()),
                            execution_id: agent_instance_id.clone(),
                            agent_instance_id: agent_instance_id.clone(),
                            message_id: tool.id.clone(),
                            parent_message_id: None,
                            message: Message::ToolCall {
                                id: tool.tool_call_id.clone(),
                                name: tool.tool_name.clone(),
                                arguments: tool.arguments.clone(),
                                model: tool.model.clone(),
                                provider: tool.provider.clone(),
                                timestamp: tool.timestamp.parse().ok(),
                            },
                            committed_at: tool.timestamp.parse().unwrap_or_default(),
                        },
                        agent_spec_id,
                    )
                    .map_err(commit_storage_error)?;
                Ok(())
            }
            _ => store.update_manifest(|manifest| {
                manifest.current_leaf_id = entry.leaf_target_id().map(str::to_string);
                manifest.entries.push(entry.clone());
            }),
        }
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
        let store = SessionStore::new(session_dir);
        store.update_manifest(|manifest| {
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
            let e = SessionTreeEntry::ModelChange(crate::api::ModelChangeEntry {
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
        let store = SessionStore::new(session_dir);
        store.update_manifest(|manifest| manifest.entries.extend(entries.clone()))?;
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
        let entry = SessionTreeEntry::Compaction(crate::api::CompactionEntry {
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
        let entry = SessionTreeEntry::Leaf(crate::api::LeafEntry {
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
                message: "branch-point fork is not yet supported by schema v3".into(),
            });
        }
        let source = SessionStore::new(source_dir);
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

fn commit_storage_error(error: piko_protocol::CommitError) -> SessionStorageError {
    SessionStorageError::Invalid {
        path: PathBuf::from("agent shard"),
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
    let store = SessionStore::new(dir);
    let manifest = store.load_manifest()?;
    let mut state = SessionState::new(manifest.session_id.clone(), manifest.cwd.clone());
    state.name = manifest.name.clone();
    state.current_leaf_id = manifest.current_leaf_id.clone();
    state.entries = manifest.entries.clone();
    for agent_instance_id in store.list_agents(&manifest.session_id)? {
        let recovered = store.load_agent(&manifest.session_id, &agent_instance_id)?;
        if let Some(agent) = manifest.agents.get(&agent_instance_id) {
            state.tasks.insert(
                agent_instance_id.clone(),
                agent_task_state_from_manifest_entry(&manifest, agent),
            );
        }
        for entry in agent_transcript_entries(&recovered) {
            if let SessionTreeEntry::Message(message) = &entry {
                state
                    .task_heads
                    .insert(agent_instance_id.clone(), message.id.clone());
            }
            state.entries.push(entry);
        }
    }
    state.entries.sort_by_key(|e| e.timestamp().to_string());
    state.seq = state.entries.len() as u64;
    restore_agent_runtime_state(&mut state, &manifest);
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

fn restore_agent_runtime_state(state: &mut SessionState, manifest: &SessionManifest) {
    let specs = crate::domain::agents::loader::load_agents(&state.cwd);
    for agent in manifest.agents.values() {
        let spec = specs.get(&agent.identity.agent_spec_id);
        let unread_report_count = manifest
            .agent_inbox
            .iter()
            .filter(|item| {
                item.recipient_agent_instance_id == agent.identity.agent_instance_id
                    && item.consumed_at.is_none()
            })
            .count() as u32;
        let status = match agent.lifecycle {
            piko_protocol::AgentInstanceLifecycle::Open => AgentStatus::Idle,
            piko_protocol::AgentInstanceLifecycle::Closed => AgentStatus::Closed,
            piko_protocol::AgentInstanceLifecycle::Terminated => AgentStatus::Stopped,
            piko_protocol::AgentInstanceLifecycle::Unavailable => AgentStatus::Failed,
        };
        state.active_agents.insert(
            agent.identity.agent_instance_id.clone(),
            AgentInfo {
                agent_instance_id: agent.identity.agent_instance_id.clone(),
                agent_id: agent.identity.agent_spec_id.clone(),
                parent_agent_instance_id: agent.identity.parent_agent_instance_id.clone(),
                lifecycle: agent.lifecycle,
                activity: piko_protocol::AgentActivity::Idle,
                unread_report_count,
                name: spec
                    .map(|spec| spec.name.clone())
                    .unwrap_or_else(|| agent.identity.agent_spec_id.clone()),
                role: spec
                    .map(|spec| spec.role.clone())
                    .unwrap_or_else(|| "assistant".into()),
                status,
            },
        );
    }

    state.active_agent_instance_id = state
        .active_agents
        .values()
        .find(|agent| agent.parent_agent_instance_id.is_none())
        .map(|agent| agent.agent_instance_id.clone())
        .or_else(|| state.active_agents.keys().next().cloned());

    let entries = state.entries.clone();
    for entry in entries {
        for (agent_instance_id, agent_id, message) in
            project_agent_view_from_entry(&state.session_id, &entry)
        {
            let seq = state.next_agent_view_seq;
            state.next_agent_view_seq = state.next_agent_view_seq.saturating_add(1);
            let view = state
                .agent_views
                .entry(agent_instance_id.clone())
                .or_insert_with(|| AgentViewState {
                    agent_instance_id: agent_instance_id.clone(),
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
            let agent_instance_id = &message.agent_instance_id;
            let agent_id = &message.agent_id;
            match &message.message {
                Message::User { .. } | Message::Assistant { .. } | Message::ToolResult { .. } => {
                    vec![(
                        agent_instance_id.clone(),
                        agent_id.clone(),
                        ServerMessage::TranscriptCommitted(
                            piko_protocol::TranscriptCommittedEvent {
                                session_id: session_id.to_string(),
                                agent_instance_id: agent_instance_id.clone(),
                                agent_id: agent_id.clone(),
                                source_turn_id: message.source_turn_id.clone(),
                                message_id: message.id.clone(),
                                transcript_seq: message.transcript_seq,
                                message: message.message.clone(),
                            },
                        ),
                    )]
                }
                _ => Vec::new(),
            }
        }
        SessionTreeEntry::ToolCall(tool) => {
            let (Some(agent_instance_id), Some(agent_id)) = (&tool.task_id, &tool.agent_id) else {
                return Vec::new();
            };
            vec![(
                agent_instance_id.clone(),
                agent_id.clone(),
                ServerMessage::ToolExecution(piko_protocol::ToolExecutionEvent::Started {
                    task_id: agent_instance_id.clone(),
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
