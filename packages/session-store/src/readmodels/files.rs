use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::io_error;
use crate::journal::{DurableCommit, SessionIdentityFile};
use crate::journal_io::sync_dir;
use crate::{Result, StoreError};

pub(crate) fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .map_err(|source| io_error(&tmp, source))?;
    serde_json::to_writer(&mut file, value).map_err(|source| StoreError::Json {
        path: tmp.clone(),
        source,
    })?;
    file.write_all(b"\n")
        .and_then(|_| file.sync_all())
        .map_err(|source| io_error(&tmp, source))?;
    fs::rename(&tmp, path).map_err(|source| io_error(path, source))?;
    sync_dir(path.parent().unwrap_or_else(|| Path::new(".")))
}

pub const READ_MODEL_SCHEMA: u32 = 1;

pub(crate) fn dir(path: &Path) -> PathBuf {
    path.join("readmodels")
}

pub(crate) fn head_path(path: &Path) -> PathBuf {
    dir(path).join("head.json")
}

pub(crate) fn catalog_path(path: &Path) -> PathBuf {
    dir(path).join("catalog.json")
}

pub(crate) fn current_path(path: &Path) -> PathBuf {
    dir(path).join("current.json")
}

pub(crate) fn trajectory_path(path: &Path) -> PathBuf {
    dir(path).join("trajectory.json")
}

pub(crate) fn ensure_dir(path: &Path) -> Result<()> {
    let directory = dir(path);
    if directory.exists() {
        return Ok(());
    }
    fs::create_dir(&directory).map_err(|source| io_error(&directory, source))
}

pub(crate) fn load_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(path, error)),
    }
}

pub(crate) fn envelope_matches(
    schema_version: u32,
    session_id: &str,
    journal_generation: &str,
    through_revision: u64,
    through_checksum: &str,
    identity: &SessionIdentityFile,
    tip: &DurableCommit,
) -> bool {
    schema_version == READ_MODEL_SCHEMA
        && session_id == identity.session_id
        && journal_generation == identity.journal_generation
        && through_revision == tip.revision
        && through_checksum == tip.checksum.value
}
