use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::io_error;
use crate::journal::DurableCommit;
use crate::journal_io::sync_dir;
use crate::{COMMITS_PER_SEGMENT, Result, StoreError};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SegmentDescriptor {
    pub start: u64,
    pub end: Option<u64>,
}

pub(crate) fn descriptor(path: &Path) -> Result<SegmentDescriptor> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| corrupt(path, "invalid segment filename"))?;
    let stem = name
        .strip_suffix(".jsonl")
        .ok_or_else(|| corrupt(path, "segment must use .jsonl"))?;
    let (start, end) = stem
        .split_once('-')
        .ok_or_else(|| corrupt(path, "segment filename has no range"))?;
    if start.len() != 20 || (end != "open" && end.len() != 20) {
        return Err(corrupt(path, "segment revisions must be zero-padded"));
    }
    let start = start
        .parse::<u64>()
        .map_err(|_| corrupt(path, "invalid segment start"))?;
    let end = if end == "open" {
        None
    } else {
        Some(
            end.parse::<u64>()
                .map_err(|_| corrupt(path, "invalid segment end"))?,
        )
    };
    Ok(SegmentDescriptor { start, end })
}

pub(crate) fn validate_segment(
    path: &Path,
    descriptor: SegmentDescriptor,
    position: usize,
    segment_count: usize,
    first_revision: Option<u64>,
    last_revision: Option<u64>,
    record_count: usize,
) -> Result<()> {
    let expected_start = position as u64 * COMMITS_PER_SEGMENT + 1;
    if descriptor.start != expected_start {
        return Err(corrupt(path, "segment sequence is not contiguous"));
    }
    if descriptor.end.is_none() && position + 1 != segment_count {
        return Err(corrupt(path, "open segment is not last"));
    }
    if let Some(end) = descriptor.end {
        if end != descriptor.start + COMMITS_PER_SEGMENT - 1
            || record_count != COMMITS_PER_SEGMENT as usize
            || first_revision != Some(descriptor.start)
            || last_revision != Some(end)
        {
            return Err(corrupt(
                path,
                "closed segment does not match its fixed range",
            ));
        }
    // A crash can happen after the boundary commit is synced but before the
    // open segment is renamed to its closed range. Accept that one exact,
    // verified boundary here; `SessionStore::open` normalizes its filename
    // before admitting another append.
    } else if record_count > COMMITS_PER_SEGMENT as usize
        || first_revision.is_some_and(|revision| revision != descriptor.start)
    {
        return Err(corrupt(path, "invalid open segment range"));
    }
    Ok(())
}

fn corrupt(path: &Path, message: &str) -> StoreError {
    StoreError::Corruption {
        path: path.to_path_buf(),
        line: 0,
        message: message.into(),
    }
}

pub(crate) fn segment_start(revision: u64) -> u64 {
    ((revision.saturating_sub(1)) / COMMITS_PER_SEGMENT) * COMMITS_PER_SEGMENT + 1
}

pub(crate) fn create_open_segment(path: &Path, start: u64) -> Result<()> {
    let segment = path.join("events").join(format!("{start:020}-open.jsonl"));
    File::create(&segment)
        .and_then(|file| file.sync_all())
        .map_err(|source| io_error(&segment, source))?;
    sync_dir(&path.join("events"))
}

pub(crate) fn normalize_segment_boundary(path: &Path, revision: u64) -> Result<bool> {
    let events = path.join("events");
    let mut repaired = false;
    if revision == 0 {
        let open = events.join(format!("{:020}-open.jsonl", 1));
        if !open.exists() {
            create_open_segment(path, 1)?;
            repaired = true;
        }
        return Ok(repaired);
    }
    if !revision.is_multiple_of(COMMITS_PER_SEGMENT) {
        return Ok(false);
    }
    let start = segment_start(revision);
    let open = events.join(format!("{start:020}-open.jsonl"));
    let closed = events.join(format!("{start:020}-{revision:020}.jsonl"));
    if open.exists() && !closed.exists() {
        fs::rename(&open, &closed).map_err(|source| io_error(&open, source))?;
        sync_dir(&events)?;
        repaired = true;
    }
    let next = events.join(format!("{:020}-open.jsonl", revision + 1));
    if !next.exists() {
        create_open_segment(path, revision + 1)?;
        repaired = true;
    }
    Ok(repaired)
}

pub(crate) fn append_line(path: &Path, commit: &DurableCommit) -> Result<()> {
    let mut encoded = serde_json::to_vec(commit).map_err(|source| StoreError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    encoded.push(b'\n');
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|source| io_error(path, source))?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_data())
        .map_err(|source| io_error(path, source))
}

pub(crate) fn open_path(path: &Path, revision: u64) -> PathBuf {
    path.join("events")
        .join(format!("{:020}-open.jsonl", segment_start(revision)))
}
