use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions as FsOpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use piko_protocol::AgentInstanceIdentity;
use serde::{Deserialize, Serialize};

use crate::error::io_error;
use crate::journal_io::{checksum, proposal_matches, sync_dir};
use crate::replay::read_all;
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
    /// Observational event view, including ignorable types. Populated on
    /// open from the journal and extended on each successful append so
    /// readers do not re-scan JSONL.
    pub(crate) raw_events: Mutex<Vec<crate::RawJournalEvent>>,
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

    /// Lightweight list-summary load: boundary snapshot plus the open tail
    /// segment only. Returns `None` when the session has no boundary snapshot
    /// yet (caller falls back to a full open). Does not take the writer lock,
    /// repair the tail, or retain a journal handle, so it is cheap enough to
    /// run for every listed session.
    pub fn inspect_facts(path: &Path) -> Result<Option<crate::journal_queries::JournalFacts>> {
        let path = path
            .canonicalize()
            .map_err(|source| io_error(path, source))?;
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
        let Some(snapshot) = crate::snapshot::load_for_replay(
            &path,
            &identity.session_id,
            &identity.journal_generation,
        ) else {
            return Ok(None);
        };
        let boundary = snapshot.aggregate.revision;
        // Byte-level verification of snapshot-covered closed segments. Full
        // deserialization of those events is skipped, but checksums and the
        // revision/previous-checksum chain still catch corruption.
        if boundary > 0
            && let Some(last) = crate::replay::verify_closed_segments_checksums(&path)?
            && last != snapshot.through_commit_checksum
        {
            return Err(StoreError::InvalidEvent(
                "journal closed segments do not match snapshot".into(),
            ));
        }
        let events_dir = path.join("events");
        let mut tail = Vec::new();
        let mut recovery = RecoveryReport::default();
        let mut saw_open = false;
        for entry in fs::read_dir(&events_dir).map_err(|source| io_error(&events_dir, source))? {
            let entry = entry.map_err(|source| io_error(&events_dir, source))?;
            let segment = entry.path();
            if segment.extension().and_then(|value| value.to_str()) != Some("jsonl") {
                continue;
            }
            if !segment
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("-open.jsonl"))
            {
                continue;
            }
            crate::replay::read_segment(
                &segment,
                false,
                &mut tail,
                &mut recovery,
                Some((boundary, snapshot.through_commit_checksum.as_str())),
            )?;
            saw_open = true;
        }
        if saw_open {
            if let Some(first) = tail.first() {
                if first.revision != boundary + 1 {
                    return Err(StoreError::Corruption {
                        path: path.clone(),
                        line: 1,
                        message: format!(
                            "tail starts at revision {}, expected {}",
                            first.revision,
                            boundary + 1
                        ),
                    });
                }
                if first.previous_checksum.as_deref()
                    != Some(snapshot.through_commit_checksum.as_str())
                {
                    return Err(StoreError::InvalidEvent(
                        "journal tail does not continue from snapshot".into(),
                    ));
                }
            }
            let mut expected = snapshot.through_commit_checksum.clone();
            for (index, commit) in tail.iter().enumerate() {
                if commit.session_id != identity.session_id
                    || commit.journal_generation != identity.journal_generation
                {
                    return Err(StoreError::InvalidEvent(
                        "journal identity/generation mismatch in tail".into(),
                    ));
                }
                if commit.previous_checksum.as_deref() != Some(expected.as_str()) {
                    return Err(StoreError::InvalidEvent(
                        "journal tail checksum chain mismatch".into(),
                    ));
                }
                expected = commit.checksum.value.clone();
                if index > 0 && commit.revision != tail[index - 1].revision + 1 {
                    return Err(StoreError::InvalidEvent("journal tail revision gap".into()));
                }
            }
        }
        let mut aggregate = snapshot.aggregate;
        for commit in &tail {
            aggregate.apply_for_replay(commit)?;
        }
        Ok(Some(crate::journal_queries::facts_from_aggregate(
            &aggregate,
        )))
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
        let (commits, mut recovery, _) = read_all(&path, options.repair_incomplete_tail)?;
        if commits.is_empty() && !allow_empty_genesis {
            return Err(StoreError::InvalidEvent(
                "journal is missing the session_created genesis commit".into(),
            ));
        }
        let last_checksum = commits.last().map(|commit| commit.checksum.value.clone());
        if commits.iter().any(|commit| {
            commit.session_id != identity.session_id
                || commit.journal_generation != identity.journal_generation
        }) {
            return Err(StoreError::InvalidEvent(
                "journal identity/generation mismatch".into(),
            ));
        }
        let mut aggregate = crate::snapshot::load_for_replay(
            &path,
            &identity.session_id,
            &identity.journal_generation,
        )
        .filter(|snapshot| {
            commits
                .get(snapshot.aggregate.revision.saturating_sub(1) as usize)
                .is_some_and(|commit| {
                    commit.revision == snapshot.aggregate.revision
                        && commit.checksum.value == snapshot.through_commit_checksum
                })
        })
        .map(|snapshot| snapshot.aggregate)
        .unwrap_or_default();
        let snapshot_revision = aggregate.revision;
        for commit in commits
            .iter()
            .filter(|commit| commit.revision > snapshot_revision)
        {
            aggregate.apply_for_replay(commit)?;
        }
        if aggregate.revision > 0
            && (aggregate.session_id.as_deref() != Some(identity.session_id.as_str())
                || aggregate.cwd.as_deref() != Some(identity.cwd.as_str()))
        {
            return Err(StoreError::InvalidEvent(
                "journal aggregate does not match session identity".into(),
            ));
        }
        if normalize_segment_boundary(&path, aggregate.revision)? {
            recovery.repaired = true;
        }
        let boundary_revision = (aggregate.revision / COMMITS_PER_SEGMENT) * COMMITS_PER_SEGMENT;
        let boundary_snapshot = if boundary_revision == 0 {
            None
        } else {
            let boundary_checksum = commits[(boundary_revision - 1) as usize]
                .checksum
                .value
                .clone();
            if crate::snapshot::valid_boundary(
                &path,
                &identity.session_id,
                &identity.journal_generation,
                boundary_revision,
                &boundary_checksum,
            ) {
                None
            } else {
                let boundary_aggregate = if aggregate.revision == boundary_revision {
                    aggregate.clone()
                } else {
                    let mut rebuilt = SessionAggregate::default();
                    for commit in commits.iter().take(boundary_revision as usize) {
                        rebuilt.apply_for_replay(commit)?;
                    }
                    rebuilt
                };
                Some((boundary_aggregate, boundary_checksum))
            }
        };
        let raw_events = crate::journal_queries::events_from_commits(&commits);
        let inner = Arc::new(SessionStoreInner {
            path: path.clone(),
            session_id: identity.session_id,
            journal_generation: identity.journal_generation,
            _lock: lock,
            aggregate: Mutex::new(aggregate.clone()),
            last_checksum: Mutex::new(last_checksum),
            raw_events: Mutex::new(raw_events),
        });
        registry.insert(path, Arc::downgrade(&inner));
        if let Some((boundary_aggregate, checksum)) = boundary_snapshot {
            crate::snapshot::schedule(
                inner.path.clone(),
                inner.session_id.clone(),
                inner.journal_generation.clone(),
                boundary_aggregate,
                checksum,
            );
        }
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
                if revision.is_multiple_of(COMMITS_PER_SEGMENT) {
                    crate::snapshot::schedule(
                        self.inner.path.clone(),
                        self.inner.session_id.clone(),
                        self.inner.journal_generation.clone(),
                        aggregate.clone(),
                        existing.checksum.value.clone(),
                    );
                }
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
        self.inner
            .raw_events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .extend(crate::journal_queries::events_from_commit(&commit));
        if revision.is_multiple_of(COMMITS_PER_SEGMENT) {
            self.roll_segment(revision)?;
            crate::snapshot::schedule(
                self.inner.path.clone(),
                self.inner.session_id.clone(),
                self.inner.journal_generation.clone(),
                aggregate.clone(),
                commit.checksum.value.clone(),
            );
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
