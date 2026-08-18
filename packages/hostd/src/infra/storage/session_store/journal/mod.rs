use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use piko_session_store::{
    EventData, NewSession, OpenOptions, ProposedCommit, RawEvent, SessionAggregate,
    SessionStore as Journal,
};

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
