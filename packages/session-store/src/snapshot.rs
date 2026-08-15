use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::io_error;
use crate::journal_io::checksum_record;
use crate::{Result, SCHEMA_VERSION, SessionAggregate, SessionStore, StoreError};

const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotFile {
    schema_version: u32,
    session_schema_version: u32,
    session_id: String,
    journal_generation: String,
    through_revision: u64,
    through_commit_checksum: String,
    aggregate: SessionAggregate,
    checksum: String,
    #[serde(default)]
    extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRef {
    pub path: PathBuf,
    pub through_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotStatus {
    Missing,
    Valid(SnapshotRef),
    Invalid { path: PathBuf, message: String },
}

impl SessionStore {
    pub fn latest_snapshot(&self) -> SnapshotStatus {
        match load_latest(self.path(), self.session_id(), self.journal_generation()) {
            Ok(Some((snapshot, path))) => SnapshotStatus::Valid(SnapshotRef {
                path,
                through_revision: snapshot.through_revision,
            }),
            Ok(None) => SnapshotStatus::Missing,
            Err(StoreError::Corruption { path, message, .. }) => {
                SnapshotStatus::Invalid { path, message }
            }
            Err(error) => SnapshotStatus::Invalid {
                path: self.path().join("snapshots"),
                message: error.to_string(),
            },
        }
    }

    /// Explicit snapshot creation for diagnostics and deterministic tests.
    /// Normal 1,000-commit boundaries schedule the same operation in the
    /// background after segment rollover succeeds.
    pub fn write_snapshot(&self) -> Result<SnapshotRef> {
        let aggregate = self.aggregate();
        if !aggregate
            .revision
            .is_multiple_of(crate::COMMITS_PER_SEGMENT)
        {
            return Err(StoreError::InvalidEvent(format!(
                "snapshot revision {} is not a segment boundary",
                aggregate.revision
            )));
        }
        let commit = self.commit_at(aggregate.revision)?;
        write(
            self.path(),
            self.session_id(),
            self.journal_generation(),
            aggregate,
            commit.checksum.value,
        )
    }
}

pub(crate) fn schedule(
    path: PathBuf,
    session_id: String,
    journal_generation: String,
    aggregate: SessionAggregate,
    through_commit_checksum: String,
) {
    std::thread::Builder::new()
        .name(format!("piko-snapshot-{}", aggregate.revision))
        .spawn(move || {
            // Snapshot is disposable. A failure leaves journal correctness and
            // the previous snapshot untouched; a later boundary/open can retry.
            let _ = write(
                &path,
                &session_id,
                &journal_generation,
                aggregate,
                through_commit_checksum,
            );
        })
        .ok();
}

pub(crate) fn valid_boundary(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
    revision: u64,
    through_commit_checksum: &str,
) -> bool {
    let snapshot_path = path.join("snapshots").join(format!("{revision:020}.json"));
    load_one(snapshot_path, session_id, journal_generation).is_ok_and(|snapshot| {
        snapshot.through_revision == revision
            && snapshot.through_commit_checksum == through_commit_checksum
    })
}

pub(crate) struct SnapshotCandidate {
    pub aggregate: SessionAggregate,
    pub through_commit_checksum: String,
}

pub(crate) fn load_for_replay(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
) -> Option<SnapshotCandidate> {
    snapshot_paths(path)
        .ok()?
        .into_iter()
        .rev()
        .find_map(|path| {
            let snapshot = load_one(path, session_id, journal_generation).ok()?;
            Some(SnapshotCandidate {
                aggregate: snapshot.aggregate,
                through_commit_checksum: snapshot.through_commit_checksum,
            })
        })
}

fn write(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
    aggregate: SessionAggregate,
    through_commit_checksum: String,
) -> Result<SnapshotRef> {
    let through_revision = aggregate.revision;
    let destination = path
        .join("snapshots")
        .join(format!("{through_revision:020}.json"));
    let mut snapshot = SnapshotFile {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        session_schema_version: SCHEMA_VERSION,
        session_id: session_id.to_string(),
        journal_generation: journal_generation.to_string(),
        through_revision,
        through_commit_checksum,
        aggregate,
        checksum: String::new(),
        extensions: BTreeMap::new(),
    };
    snapshot.checksum = checksum(&snapshot)?;
    let tmp = destination.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .map_err(|source| io_error(&tmp, source))?;
    serde_json::to_writer(&mut file, &snapshot).map_err(|source| StoreError::Json {
        path: tmp.clone(),
        source,
    })?;
    file.write_all(b"\n")
        .and_then(|_| file.sync_all())
        .map_err(|source| io_error(&tmp, source))?;
    fs::rename(&tmp, &destination).map_err(|source| io_error(&destination, source))?;
    sync_dir(&path.join("snapshots"))?;
    Ok(SnapshotRef {
        path: destination,
        through_revision,
    })
}

fn load_latest(
    path: &Path,
    session_id: &str,
    journal_generation: &str,
) -> Result<Option<(SnapshotFile, PathBuf)>> {
    let Some(path) = snapshot_paths(path)?.pop() else {
        return Ok(None);
    };
    let snapshot = load_one(path.clone(), session_id, journal_generation)?;
    Ok(Some((snapshot, path)))
}

fn snapshot_paths(path: &Path) -> Result<Vec<PathBuf>> {
    let directory = path.join("snapshots");
    let mut snapshots = fs::read_dir(&directory)
        .map_err(|source| io_error(&directory, source))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    snapshots.sort();
    Ok(snapshots)
}

fn load_one(path: PathBuf, session_id: &str, journal_generation: &str) -> Result<SnapshotFile> {
    let data = fs::read(&path).map_err(|source| io_error(&path, source))?;
    let snapshot: SnapshotFile =
        serde_json::from_slice(&data).map_err(|source| StoreError::Corruption {
            path: path.clone(),
            line: 1,
            message: source.to_string(),
        })?;
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION
        || snapshot.session_schema_version != SCHEMA_VERSION
        || snapshot.session_id != session_id
        || snapshot.journal_generation != journal_generation
        || snapshot.through_revision != snapshot.aggregate.revision
    {
        return Err(StoreError::Corruption {
            path,
            line: 1,
            message: "snapshot identity/version mismatch".into(),
        });
    }
    let record = data
        .strip_suffix(b"\n")
        .and_then(|record| record.strip_suffix(b"\r").or(Some(record)))
        .unwrap_or(data.as_slice());
    if checksum_record(record, b"\"\"").as_deref() != Some(snapshot.checksum.as_str()) {
        return Err(StoreError::Corruption {
            path,
            line: 1,
            message: "snapshot checksum mismatch".into(),
        });
    }
    Ok(snapshot)
}

fn checksum(snapshot: &SnapshotFile) -> Result<String> {
    let mut unsigned = snapshot.clone();
    unsigned.checksum.clear();
    let bytes = serde_json::to_vec(&unsigned).map_err(|source| StoreError::Json {
        path: "snapshot checksum".into(),
        source,
    })?;
    Ok(format!("{:08x}", crc32fast::hash(&bytes)))
}

fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|source| io_error(path, source))
}
