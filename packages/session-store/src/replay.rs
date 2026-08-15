use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::error::io_error;
use crate::journal::{DurableCommit, RecoveryReport};
use crate::journal_io::checksum_record;
use crate::segments::{descriptor, validate_segment};
use crate::{Result, SCHEMA_VERSION, StoreError};

pub(crate) fn read_all(
    path: &Path,
    repair: bool,
) -> Result<(Vec<DurableCommit>, RecoveryReport, usize)> {
    let events = path.join("events");
    let mut segments = fs::read_dir(&events)
        .map_err(|source| io_error(&events, source))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
        .collect::<Vec<_>>();
    segments.sort();
    let mut commits = Vec::new();
    let mut recovery = RecoveryReport::default();
    for (position, segment) in segments.iter().enumerate() {
        let descriptor = descriptor(segment)?;
        let before = commits.len();
        read_segment(
            segment,
            repair && position + 1 == segments.len(),
            &mut commits,
            &mut recovery,
        )?;
        let added = &commits[before..];
        validate_segment(
            segment,
            descriptor,
            position,
            segments.len(),
            added.first().map(|commit| commit.revision),
            added.last().map(|commit| commit.revision),
            added.len(),
        )?;
    }
    recovery.last_verified_revision = commits.last().map_or(0, |commit| commit.revision);
    Ok((commits, recovery, segments.len()))
}

fn read_segment(
    path: &Path,
    repair: bool,
    commits: &mut Vec<DurableCommit>,
    recovery: &mut RecoveryReport,
) -> Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(repair)
        .open(path)
        .map_err(|source| io_error(path, source))?;
    let original_len = file
        .metadata()
        .map_err(|source| io_error(path, source))?
        .len();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|source| io_error(path, source))?;
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        if !repair {
            return Err(StoreError::Corruption {
                path: path.to_path_buf(),
                line: bytes.iter().filter(|byte| **byte == b'\n').count() + 1,
                message: "incomplete final record".into(),
            });
        }
        let keep = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |position| position + 1);
        file.set_len(keep as u64)
            .and_then(|_| file.seek(SeekFrom::Start(keep as u64)).map(|_| ()))
            .and_then(|_| file.sync_data())
            .map_err(|source| io_error(path, source))?;
        recovery.repaired = true;
        recovery.truncated_bytes += original_len - keep as u64;
        bytes.truncate(keep);
    }
    for (index, line) in BufReader::new(bytes.as_slice()).lines().enumerate() {
        let line = line.map_err(|source| io_error(path, source))?;
        if line.is_empty() {
            continue;
        }
        let commit: DurableCommit =
            serde_json::from_str(&line).map_err(|source| StoreError::Corruption {
                path: path.to_path_buf(),
                line: index + 1,
                message: source.to_string(),
            })?;
        if commit.schema_version != SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchema {
                found: commit.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        let unsigned_checksum = serde_json::to_vec(&crate::journal::Checksum {
            algorithm: commit.checksum.algorithm.clone(),
            value: String::new(),
        })
        .map_err(|source| StoreError::Json {
            path: "journal checksum".into(),
            source,
        })?;
        if checksum_record(line.as_bytes(), &unsigned_checksum).as_deref()
            != Some(commit.checksum.value.as_str())
        {
            return Err(StoreError::Corruption {
                path: path.to_path_buf(),
                line: index + 1,
                message: "checksum mismatch".into(),
            });
        }
        let expected_previous = commits.last().map(|prior| prior.checksum.value.as_str());
        if commit.previous_checksum.as_deref() != expected_previous {
            return Err(StoreError::Corruption {
                path: path.to_path_buf(),
                line: index + 1,
                message: "commit checksum chain mismatch".into(),
            });
        }
        let expected = commits.last().map_or(1, |prior| prior.revision + 1);
        if commit.revision != expected {
            return Err(StoreError::Corruption {
                path: path.to_path_buf(),
                line: index + 1,
                message: format!("expected revision {expected}, got {}", commit.revision),
            });
        }
        commits.push(commit);
    }
    Ok(())
}
