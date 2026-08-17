use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions as FsOpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use piko_protocol::AgentInstanceIdentity;
use serde::{Deserialize, Serialize};

use crate::error::io_error;
use crate::journal_io::{checksum, proposal_matches, sync_dir};

use crate::schema::RawEvent;
use crate::segments::{
    append_line, create_open_segment, normalize_segment_boundary, open_path, segment_start,
};
use crate::{COMMITS_PER_SEGMENT, Result, SCHEMA_VERSION, SessionAggregate, StoreError};

#[derive(Debug, Clone)]
pub struct NewSession {
    pub session_id: String,
    pub cwd: String,
    pub created_at: i64,
    pub root: AgentInstanceIdentity,
}

#[derive(Debug, Clone, Copy)]
pub struct OpenOptions {
    pub repair_incomplete_tail: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            repair_incomplete_tail: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProposedCommit {
    pub commit_id: String,
    pub committed_at: i64,
    pub causation_id: Option<String>,
    pub correlation_id: Option<String>,
    pub events: Vec<RawEvent>,
    pub extensions: BTreeMap<String, serde_json::Value>,
}

impl ProposedCommit {
    pub fn one(commit_id: impl Into<String>, committed_at: i64, event: RawEvent) -> Self {
        Self {
            commit_id: commit_id.into(),
            committed_at,
            causation_id: None,
            correlation_id: None,
            events: vec![event],
            extensions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DurableCommit {
    pub schema_version: u32,
    pub session_id: String,
    pub journal_generation: String,
    pub revision: u64,
    pub commit_id: String,
    pub committed_at: i64,
    pub producer: Producer,
    pub causation_id: Option<String>,
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub previous_checksum: Option<String>,
    pub events: Vec<RawEvent>,
    #[serde(default)]
    pub extensions: BTreeMap<String, serde_json::Value>,
    pub checksum: Checksum,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Producer {
    pub component: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Checksum {
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionIdentityFile {
    pub(crate) schema_version: u32,
    pub(crate) session_id: String,
    pub(crate) cwd: String,
    pub(crate) created_at: i64,
    pub(crate) journal_generation: String,
    #[serde(default)]
    pub(crate) extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub repaired: bool,
    pub truncated_bytes: u64,
    pub last_verified_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDescriptor {
    pub session_id: String,
    pub cwd: String,
    pub created_at: i64,
}

#[derive(Debug)]
pub struct OpenedSession {
    pub store: SessionStore,
    pub aggregate: SessionAggregate,
    pub recovery: RecoveryReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub revision: u64,
    pub segment_count: usize,
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    pub(crate) inner: Arc<SessionStoreInner>,
}

#[derive(Debug)]
pub(crate) struct SessionStoreInner {
    pub(crate) path: PathBuf,
    pub(crate) session_id: String,
    pub(crate) journal_generation: String,
    _lock: File,
    pub(crate) aggregate: Mutex<SessionAggregate>,
    last_checksum: Mutex<Option<String>>,
    pub(crate) trajectory: Mutex<crate::TrajectoryProjection>,
}

static OPEN_STORES: OnceLock<Mutex<BTreeMap<PathBuf, Weak<SessionStoreInner>>>> = OnceLock::new();

impl SessionStore {
    pub fn inspect(path: &Path) -> Result<SessionDescriptor> {
        let identity_path = path.join("session.json");
        let data = fs::read(&identity_path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound(path.to_path_buf())
            } else {
                io_error(&identity_path, source)
            }
        })?;
        let identity: SessionIdentityFile =
            serde_json::from_slice(&data).map_err(|source| StoreError::Json {
                path: identity_path,
                source,
            })?;
        if identity.schema_version != SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchema {
                found: identity.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        Ok(SessionDescriptor {
            session_id: identity.session_id,
            cwd: identity.cwd,
            created_at: identity.created_at,
        })
    }

    pub fn open(path: &Path, options: OpenOptions) -> Result<OpenedSession> {
        Self::open_internal(path, options, false)
    }

    pub(crate) fn open_internal(
        path: &Path,
        options: OpenOptions,
        allow_empty_genesis: bool,
    ) -> Result<OpenedSession> {
        let path = path
            .canonicalize()
            .map_err(|source| io_error(path, source))?;
        let identity_path = path.join("session.json");
        let identity_data = fs::read(&identity_path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound(path.to_path_buf())
            } else {
                io_error(&identity_path, source)
            }
        })?;
        let identity: SessionIdentityFile =
            serde_json::from_slice(&identity_data).map_err(|source| StoreError::Json {
                path: identity_path,
                source,
            })?;
        if identity.schema_version != SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchema {
                found: identity.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        // Serialize same-process open through lock acquisition and replay.
        let mut registry = open_stores()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(inner) = registry.get(&path).and_then(Weak::upgrade) {
            let store = Self { inner };
            return Ok(OpenedSession {
                aggregate: store.aggregate(),
                store,
                recovery: RecoveryReport::default(),
            });
        }
        let lock_path = path.join("writer.lock");
        let lock = FsOpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|source| io_error(&lock_path, source))?;
        lock.try_lock().map_err(|source| match source {
            std::fs::TryLockError::WouldBlock => StoreError::WriterLocked(lock_path.clone()),
            std::fs::TryLockError::Error(source) => io_error(&lock_path, source),
        })?;
        let (aggregate, trajectory, last_checksum, recovery) =
            crate::readmodels::load_or_rebuild(&path, &identity, options, allow_empty_genesis)?;
        let inner = Arc::new(SessionStoreInner {
            path: path.clone(),
            session_id: identity.session_id,
            journal_generation: identity.journal_generation,
            _lock: lock,
            aggregate: Mutex::new(aggregate.clone()),
            last_checksum: Mutex::new(last_checksum),
            trajectory: Mutex::new(trajectory),
        });
        registry.insert(path, Arc::downgrade(&inner));
        Ok(OpenedSession {
            store: Self { inner },
            aggregate,
            recovery,
        })
    }

    pub fn append(
        &self,
        expected_revision: u64,
        proposed: ProposedCommit,
    ) -> Result<DurableCommit> {
        let mut aggregate = self
            .inner
            .aggregate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(revision) = aggregate.commit_revision(&proposed.commit_id) {
            if revision.is_multiple_of(COMMITS_PER_SEGMENT) {
                let _ = normalize_segment_boundary(&self.inner.path, revision)?;
            }
            let existing = self.commit_at(revision)?;
            if proposal_matches(&proposed, &existing) {
                let trajectory = self
                    .inner
                    .trajectory
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .clone();
                let _ = crate::readmodels::publish(
                    &self.inner.path,
                    &self.inner.session_id,
                    &self.inner.journal_generation,
                    &aggregate,
                    &trajectory,
                    &existing.checksum.value,
                );
                return Ok(existing);
            }
            return Err(StoreError::IdempotencyConflict(proposed.commit_id));
        }
        if expected_revision != aggregate.revision {
            return Err(StoreError::RevisionConflict {
                expected: expected_revision,
                current: aggregate.revision,
            });
        }
        if proposed.events.is_empty() {
            return Err(StoreError::InvalidEvent("commit has no events".into()));
        }
        let revision = expected_revision + 1;
        let previous_checksum = self
            .inner
            .last_checksum
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let mut commit = DurableCommit {
            schema_version: SCHEMA_VERSION,
            session_id: self.inner.session_id.clone(),
            journal_generation: self.inner.journal_generation.clone(),
            revision,
            commit_id: proposed.commit_id,
            committed_at: proposed.committed_at,
            producer: Producer {
                component: "hostd".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            causation_id: proposed.causation_id,
            correlation_id: proposed.correlation_id,
            previous_checksum,
            events: proposed.events,
            extensions: proposed.extensions,
            checksum: Checksum {
                algorithm: "crc32".into(),
                value: String::new(),
            },
        };
        commit.checksum.value = checksum(&commit)?;
        let mut preflight = aggregate.clone();
        preflight.apply(&commit)?;
        append_line(&self.open_segment_path(revision), &commit)?;
        // The commit is authoritative once append+sync succeeds. Record it in
        // memory before rollover so an unacknowledged rollover failure retries
        // idempotently instead of appending the same revision twice.
        *aggregate = preflight;
        *self
            .inner
            .last_checksum
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(commit.checksum.value.clone());
        {
            let mut trajectory = self
                .inner
                .trajectory
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            crate::readmodels::apply_trajectory_commit(&mut trajectory, &commit);
            let _ = crate::readmodels::publish(
                &self.inner.path,
                &self.inner.session_id,
                &self.inner.journal_generation,
                &aggregate,
                &trajectory,
                &commit.checksum.value,
            );
        }
        if revision.is_multiple_of(COMMITS_PER_SEGMENT) {
            self.roll_segment(revision)?;
        }
        Ok(commit)
    }

    fn open_segment_path(&self, revision: u64) -> PathBuf {
        open_path(&self.inner.path, revision)
    }

    fn roll_segment(&self, revision: u64) -> Result<()> {
        let start = segment_start(revision);
        let open = self.open_segment_path(revision);
        let closed = self
            .inner
            .path
            .join("events")
            .join(format!("{start:020}-{revision:020}.jsonl"));
        fs::rename(&open, &closed).map_err(|source| io_error(&open, source))?;
        sync_dir(&self.inner.path.join("events"))?;
        create_open_segment(&self.inner.path, revision + 1)
    }
}

fn open_stores() -> &'static Mutex<BTreeMap<PathBuf, Weak<SessionStoreInner>>> {
    OPEN_STORES.get_or_init(|| Mutex::new(BTreeMap::new()))
}
