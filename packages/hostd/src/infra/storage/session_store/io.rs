use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::Serialize;

use super::super::SessionStorageError;
use super::SessionStore;
use super::types::{AgentShardHeader, AgentShardRecord};

pub(super) fn storage_commit_error(
    error: SessionStorageError,
) -> piko_protocol::execution::CommitError {
    piko_protocol::execution::CommitError::Failed(error.to_string())
}

pub(super) fn read_records(path: &Path) -> Result<Vec<AgentShardRecord>, SessionStorageError> {
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

pub(super) fn atomic_create_jsonl(
    path: &Path,
    header: &AgentShardRecord,
) -> Result<(), SessionStorageError> {
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

pub(super) fn atomic_write_json(
    path: &Path,
    value: &impl Serialize,
) -> Result<(), SessionStorageError> {
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

impl SessionStore {
    pub(super) fn append_record(
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

    pub(super) fn read_header(&self, path: &Path) -> Result<AgentShardHeader, SessionStorageError> {
        match read_records(path)?.into_iter().next() {
            Some(AgentShardRecord::Header(header)) => Ok(header),
            _ => Err(SessionStorageError::Invalid {
                path: path.to_path_buf(),
                message: "missing agent shard header".into(),
            }),
        }
    }
}
