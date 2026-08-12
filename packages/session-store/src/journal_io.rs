use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::Serialize;

use crate::error::io_error;
use crate::{DurableCommit, ProposedCommit, Result, StoreError};

pub(crate) fn checksum(commit: &DurableCommit) -> Result<String> {
    let mut unsigned = commit.clone();
    unsigned.checksum.value.clear();
    let bytes = serde_json::to_vec(&unsigned).map_err(|source| StoreError::Json {
        path: "journal checksum".into(),
        source,
    })?;
    Ok(format!("{:08x}", crc32fast::hash(&bytes)))
}

pub(crate) fn proposal_matches(proposed: &ProposedCommit, durable: &DurableCommit) -> bool {
    proposed.commit_id == durable.commit_id
        && proposed.committed_at == durable.committed_at
        && proposed.causation_id == durable.causation_id
        && proposed.correlation_id == durable.correlation_id
        && proposed.events == durable.events
        && proposed.extensions == durable.extensions
}

pub(crate) fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .map_err(|source| io_error(&tmp, source))?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|source| StoreError::Json {
        path: tmp.clone(),
        source,
    })?;
    file.write_all(b"\n")
        .and_then(|_| file.sync_all())
        .map_err(|source| io_error(&tmp, source))?;
    fs::rename(&tmp, path).map_err(|source| io_error(path, source))?;
    sync_dir(path.parent().unwrap_or_else(|| Path::new(".")))
}

pub(crate) fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|source| io_error(path, source))
}
