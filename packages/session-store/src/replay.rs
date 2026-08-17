use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::error::io_error;
use crate::journal::{DurableCommit, RecoveryReport};
use crate::journal_io::{checksum_record, top_level_field_ranges};
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
            None,
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

/// Byte-level verification of closed (snapshot-covered) segments without
/// deserializing event payloads. Returns the last verified checksum so the
/// caller can cross-check it against the boundary snapshot. This keeps
/// lightweight list loading able to detect corruption in old segments.
pub(crate) fn verify_closed_segments_checksums(path: &Path) -> Result<Option<String>> {
    let events = path.join("events");
    let mut segments = fs::read_dir(&events)
        .map_err(|source| io_error(&events, source))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("-open.jsonl"))
        })
        .collect::<Vec<_>>();
    segments.sort();
    let mut expected_previous: Option<String> = None;
    let mut expected_revision: u64 = 1;
    let mut last_checksum: Option<String> = None;
    for segment in segments {
        let bytes = fs::read(&segment).map_err(|source| io_error(&segment, source))?;
        for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
            if line.is_empty() {
                continue;
            }
            let mut ranges =
                top_level_field_ranges(line, &[b"checksum", b"revision", b"previousChecksum"]);
            let checksum_range = ranges[0].take().ok_or_else(|| StoreError::Corruption {
                path: segment.clone(),
                line: index + 1,
                message: "missing checksum".into(),
            })?;
            let checksum_start = checksum_range.start;
            let checksum_end = checksum_range.end;
            let revision_range = ranges[1].take().ok_or_else(|| StoreError::Corruption {
                path: segment.clone(),
                line: index + 1,
                message: "missing revision".into(),
            })?;
            let previous_range = ranges[2].take();
            let checksum_value: serde_json::Value =
                serde_json::from_slice(&line[checksum_start..checksum_end]).map_err(|source| {
                    StoreError::Corruption {
                        path: segment.clone(),
                        line: index + 1,
                        message: source.to_string(),
                    }
                })?;
            let algorithm = checksum_value
                .get("algorithm")
                .and_then(|value| value.as_str())
                .unwrap_or("crc32")
                .to_string();
            let value = checksum_value
                .get("value")
                .and_then(|value| value.as_str())
                .ok_or_else(|| StoreError::Corruption {
                    path: segment.clone(),
                    line: index + 1,
                    message: "missing checksum value".into(),
                })?
                .to_string();
            let unsigned = serde_json::to_vec(&crate::journal::Checksum {
                algorithm,
                value: String::new(),
            })
            .map_err(|source| StoreError::Json {
                path: "journal checksum".into(),
                source,
            })?;
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(&line[..checksum_start]);
            hasher.update(&unsigned);
            hasher.update(&line[checksum_end..]);
            if format!("{:08x}", hasher.finalize()) != value {
                return Err(StoreError::Corruption {
                    path: segment.clone(),
                    line: index + 1,
                    message: "checksum mismatch".into(),
                });
            }
            let revision: u64 =
                serde_json::from_slice(&line[revision_range]).map_err(|source| {
                    StoreError::Corruption {
                        path: segment.clone(),
                        line: index + 1,
                        message: source.to_string(),
                    }
                })?;
            if revision != expected_revision {
                return Err(StoreError::Corruption {
                    path: segment.clone(),
                    line: index + 1,
                    message: format!("expected revision {expected_revision}, got {revision}"),
                });
            }
            let previous = previous_range
                .map(|range| serde_json::from_slice::<Option<String>>(&line[range]))
                .transpose()
                .map_err(|source| StoreError::Corruption {
                    path: segment.clone(),
                    line: index + 1,
                    message: source.to_string(),
                })?
                .flatten();
            if previous.as_deref() != expected_previous.as_deref() {
                return Err(StoreError::Corruption {
                    path: segment.clone(),
                    line: index + 1,
                    message: "checksum chain mismatch".into(),
                });
            }
            expected_previous = Some(value.clone());
            expected_revision += 1;
            last_checksum = Some(value);
        }
    }
    Ok(last_checksum)
}

pub(crate) fn read_segment(
    path: &Path,
    repair: bool,
    commits: &mut Vec<DurableCommit>,
    recovery: &mut RecoveryReport,
    seed: Option<(u64, &str)>,
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
        let expected_previous = if commits.is_empty() {
            seed.map(|(_, checksum)| checksum)
        } else {
            commits.last().map(|prior| prior.checksum.value.as_str())
        };
        if commit.previous_checksum.as_deref() != expected_previous {
            return Err(StoreError::Corruption {
                path: path.to_path_buf(),
                line: index + 1,
                message: "commit checksum chain mismatch".into(),
            });
        }
        let expected = if commits.is_empty() {
            seed.map_or(1, |(revision, _)| revision + 1)
        } else {
            commits.last().map_or(1, |prior| prior.revision + 1)
        };
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
