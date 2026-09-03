use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use piko_session_store::{
    CompactionRecordedV1, EventData, NewSession, OpenOptions, ProposedCommit, RawEvent,
    SessionAggregate, SessionStore as Journal,
};

use crate::api::SessionTreeEntry;
use crate::ports::storage_types::SessionStorageError;

mod commands;
mod fork;
pub(crate) mod mutations;
mod projection;
mod reads;
mod recovery;

#[derive(Debug, Clone)]
pub struct SessionStore {
    session_dir: PathBuf,
    pub(super) io: Arc<Mutex<()>>,
    journal: Arc<Mutex<Option<Journal>>>,
}

impl SessionStore {
    pub fn new(session_dir: impl Into<PathBuf>) -> Self {
        let session_dir = session_dir.into();
        let io = super::serial::io_lock_for(&session_dir);
        Self {
            session_dir,
            io,
            journal: Arc::new(Mutex::new(None)),
        }
    }

    pub fn create_session(
        session_dir: impl Into<PathBuf>,
        session_id: String,
        cwd: String,
        created_at: i64,
    ) -> Result<Self, SessionStorageError> {
        let store = Self::new(session_dir);
        let root = piko_protocol::AgentInstanceIdentity {
            session_id: session_id.clone(),
            agent_instance_id: format!("agent_{session_id}_root"),
            agent_spec_id: "main".into(),
            parent_agent_instance_id: None,
        };
        let opened = Journal::create(
            &store.session_dir,
            NewSession {
                session_id,
                cwd,
                created_at,
                root,
            },
        )
        .map_err(|error| store.storage_error(error))?;
        *store
            .journal
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(opened.store);
        Ok(store)
    }

    pub(super) fn journal(&self) -> Result<Journal, SessionStorageError> {
        let mut cached = self
            .journal
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(journal) = cached.as_ref() {
            return Ok(journal.clone());
        }
        let opened = Journal::open(&self.session_dir, OpenOptions::default())
            .map_err(|error| self.storage_error(error))?;
        *cached = Some(opened.store.clone());
        Ok(opened.store)
    }

    pub(crate) fn aggregate(&self) -> Result<SessionAggregate, SessionStorageError> {
        Ok(self.journal()?.aggregate())
    }

    pub fn select_branch(&self, target_id: Option<&str>) -> Result<(), SessionStorageError> {
        self.with_io(|| {
            let aggregate = self.aggregate()?;
            if aggregate.selected_tree_entry_id.as_deref() == target_id {
                return Ok(());
            }
            if let Some(id) = target_id
                && !aggregate.messages.contains_key(id)
                && !aggregate.tree_entries.contains_key(id)
            {
                return Err(self.invalid(format!("unknown tree entry: {id}")));
            }
            let root_base_message_id = root_message_ancestor(&aggregate, target_id);
            let seed = format!("{}:{}", aggregate.revision, target_id.unwrap_or("root"));
            self.commit_events(
                &piko_orchd_api::stable_internal_id(
                    "branch-selected",
                    &[aggregate.session_id.as_deref().unwrap_or_default(), &seed],
                ),
                chrono::Utc::now().timestamp_millis(),
                vec![EventData::BranchSelected {
                    selected_tree_entry_id: target_id.map(str::to_string),
                    root_base_message_id,
                }],
            )?;
            Ok(())
        })
    }

    /// Append a non-message session-tree entry and advance the selected branch
    /// atomically when that entry participates in the visible conversation.
    pub fn append_tree_entry(&self, entry: &SessionTreeEntry) -> Result<(), SessionStorageError> {
        self.with_io(|| {
            let aggregate = self.aggregate()?;
            let committed_at = entry.timestamp().parse().unwrap_or_default();
            let mut events = vec![mutations::tree_entry_event(entry)?];
            if let SessionTreeEntry::Compaction(compaction) = entry {
                events.push(EventData::CompactionRecorded(CompactionRecordedV1 {
                    compaction_id: compaction.id.clone(),
                    tree_parent_entry_id: compaction.parent_id.clone(),
                    summary: compaction.summary.clone(),
                    first_retained_entry_id: compaction.first_kept_entry_id.clone(),
                    tokens_before: compaction.tokens_before,
                    committed_at,
                }));
                events.push(EventData::WorldStateAdvanced { facts: None });
            }
            if entry.advances_selected_branch() {
                events.push(EventData::BranchSelected {
                    selected_tree_entry_id: Some(entry.id().to_string()),
                    root_base_message_id: root_message_ancestor(&aggregate, entry.parent_id()),
                });
            }
            let session_id = aggregate
                .session_id
                .as_deref()
                .ok_or_else(|| self.invalid("missing session"))?;
            let commit_id =
                piko_orchd_api::stable_internal_id("tree-entry", &[session_id, entry.id()]);
            self.commit_events(&commit_id, committed_at, events)?;
            Ok(())
        })
    }

    pub(crate) fn commit_events(
        &self,
        commit_id: &str,
        committed_at: i64,
        events: Vec<EventData>,
    ) -> Result<u64, SessionStorageError> {
        let journal = self.journal()?;
        let revision = journal.aggregate().revision;
        let raw = events
            .into_iter()
            .enumerate()
            .map(|(index, event)| {
                let index = index.to_string();
                let event_id = piko_orchd_api::stable_internal_id(
                    "session-event",
                    &[commit_id, index.as_str()],
                );
                RawEvent::new(event_id, event).map_err(|error| self.storage_error(error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let proposed = ProposedCommit {
            commit_id: commit_id.to_string(),
            committed_at,
            causation_id: None,
            correlation_id: None,
            events: raw,
            extensions: BTreeMap::new(),
        };
        journal
            .append(revision, proposed)
            .map(|commit| commit.revision)
            .map_err(|error| self.storage_error(error))
    }

    /// Append one observational trajectory record as an optional (ignorable)
    /// journal event (F-36). Best-effort: callers treat failure as a dropped
    /// record; it never affects acknowledged session facts or the turn.
    pub(crate) fn append_optional_event(
        &self,
        commit_id: &str,
        committed_at: i64,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<u64, SessionStorageError> {
        let journal = self.journal()?;
        let revision = journal.aggregate().revision;
        let raw = RawEvent::optional(commit_id, event_type, payload);
        let proposed = ProposedCommit {
            commit_id: commit_id.to_string(),
            committed_at,
            causation_id: None,
            correlation_id: None,
            events: vec![raw],
            extensions: BTreeMap::new(),
        };
        journal
            .append(revision, proposed)
            .map(|commit| commit.revision)
            .map_err(|error| self.storage_error(error))
    }

    pub fn trajectory(
        &self,
    ) -> Result<piko_session_store::TrajectoryProjection, SessionStorageError> {
        piko_session_store::query_trajectory(&self.session_dir)
            .map_err(|error| self.storage_error(error))
    }

    fn storage_error(&self, error: piko_session_store::StoreError) -> SessionStorageError {
        match error {
            piko_session_store::StoreError::NotFound(path) => {
                SessionStorageError::NotFound(path.display().to_string())
            }
            piko_session_store::StoreError::Io { path, source } => {
                SessionStorageError::Io { path, source }
            }
            piko_session_store::StoreError::Json { path, source } => {
                SessionStorageError::Json { path, source }
            }
            other => SessionStorageError::Invalid {
                path: self.session_dir.clone(),
                message: other.to_string(),
            },
        }
    }

    pub(super) fn commit_error(error: SessionStorageError) -> piko_protocol::CommitError {
        match &error {
            SessionStorageError::Invalid { message, .. }
                if message.contains("idempotency conflict") =>
            {
                piko_protocol::CommitError::IdempotencyConflict
            }
            SessionStorageError::Invalid { message, .. }
                if message.contains("unknown") || message.contains("belongs") =>
            {
                piko_protocol::CommitError::IdentityMismatch
            }
            _ => piko_protocol::CommitError::Failed(error.to_string()),
        }
    }

    pub(super) fn session_dir(&self) -> &Path {
        &self.session_dir
    }
}

/// Resolve the nearest root-AgentInstance message on a session-tree ancestry.
///
/// The session cursor may point at non-message content such as compaction or a
/// branch summary. Root transcript continuation must follow the tree ancestry
/// instead of falling back to the physically latest root message.
pub(crate) fn root_message_ancestor(
    aggregate: &SessionAggregate,
    target_id: Option<&str>,
) -> Option<String> {
    let root_id = &aggregate.root.as_ref()?.agent_instance_id;
    let mut current = target_id.map(str::to_string);
    let mut visited = std::collections::BTreeSet::new();
    while let Some(id) = current {
        if !visited.insert(id.clone()) {
            return None;
        }
        if let Some(message) = aggregate.messages.get(&id) {
            if &message.data.agent_instance_id == root_id {
                return Some(id);
            }
            current = message.data.tree_parent_entry_id.clone();
            continue;
        }
        current = aggregate
            .tree_entries
            .get(&id)
            .and_then(|entry| entry.data.parent_entry_id.clone());
    }
    None
}
