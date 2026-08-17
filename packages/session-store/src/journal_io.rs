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

/// Checksums the exact serialized bytes after replacing the top-level
/// `checksum` value with its unsigned representation.
///
/// Parsing and re-serializing can shorten an equivalent floating-point value,
/// so verification must preserve the writer's original JSON number spelling.
pub(crate) fn checksum_record(record: &[u8], unsigned_checksum: &[u8]) -> Option<String> {
    let range = top_level_field_value(record, b"checksum")?;
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&record[..range.start]);
    hasher.update(unsigned_checksum);
    hasher.update(&record[range.end..]);
    Some(format!("{:08x}", hasher.finalize()))
}

pub(crate) fn top_level_field_value(
    record: &[u8],
    target: &[u8],
) -> Option<std::ops::Range<usize>> {
    top_level_field_ranges(record, &[target])
        .into_iter()
        .next()
        .flatten()
}

/// Extract the value ranges of several top-level fields in a single pass.
/// Missing fields yield `None` entries in the returned vector.
pub(crate) fn top_level_field_ranges(
    record: &[u8],
    targets: &[&[u8]],
) -> Vec<Option<std::ops::Range<usize>>> {
    let mut result = vec![None; targets.len()];
    let mut cursor = skip_whitespace(record, 0);
    if record.get(cursor) != Some(&b'{') {
        return result;
    }
    cursor += 1;
    loop {
        cursor = skip_whitespace(record, cursor);
        if record.get(cursor) == Some(&b'}') {
            return result;
        }
        let Some((key_start, key_end, next)) = json_string(record, cursor) else {
            return result;
        };
        cursor = skip_whitespace(record, next);
        if record.get(cursor) != Some(&b':') {
            return result;
        }
        cursor = skip_whitespace(record, cursor + 1);
        let value_start = cursor;
        let Some(value_end) = json_value_end(record, cursor) else {
            return result;
        };
        for (index, target) in targets.iter().enumerate() {
            if &record[key_start..key_end] == *target {
                result[index] = Some(value_start..value_end);
            }
        }
        cursor = skip_whitespace(record, value_end);
        match record.get(cursor) {
            Some(b',') => cursor += 1,
            _ => return result,
        }
    }
}

fn json_value_end(record: &[u8], cursor: usize) -> Option<usize> {
    match record.get(cursor)? {
        b'"' => json_string(record, cursor).map(|(_, _, next)| next),
        b'{' | b'[' => composite_value_end(record, cursor),
        _ => {
            let mut end = cursor;
            while !matches!(record.get(end), None | Some(b',') | Some(b'}')) {
                end += 1;
            }
            let trimmed = record[cursor..end]
                .iter()
                .rposition(|byte| !byte.is_ascii_whitespace())?;
            Some(cursor + trimmed + 1)
        }
    }
}

fn composite_value_end(record: &[u8], cursor: usize) -> Option<usize> {
    let mut stack = vec![*record.get(cursor)?];
    let mut index = cursor + 1;
    while let Some(byte) = record.get(index) {
        match byte {
            b'"' => index = json_string(record, index)?.2,
            b'{' | b'[' => {
                stack.push(*byte);
                index += 1;
            }
            b'}' => {
                if stack.pop() != Some(b'{') {
                    return None;
                }
                index += 1;
                if stack.is_empty() {
                    return Some(index);
                }
            }
            b']' => {
                if stack.pop() != Some(b'[') {
                    return None;
                }
                index += 1;
                if stack.is_empty() {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn json_string(record: &[u8], cursor: usize) -> Option<(usize, usize, usize)> {
    if record.get(cursor) != Some(&b'"') {
        return None;
    }
    let mut index = cursor + 1;
    while let Some(byte) = record.get(index) {
        match byte {
            b'\\' => index += 2,
            b'"' => return Some((cursor + 1, index, index + 1)),
            _ => index += 1,
        }
    }
    None
}

fn skip_whitespace(record: &[u8], mut cursor: usize) -> usize {
    while record
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        cursor += 1;
    }
    cursor
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
