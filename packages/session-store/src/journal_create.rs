use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::io_error;
use crate::journal::{NewSession, OpenOptions, OpenedSession, ProposedCommit, SessionIdentityFile};
use crate::journal_io::{atomic_json, sync_dir};
use crate::schema::{EventData, RawEvent};
use crate::segments::create_open_segment;
use crate::{Result, SCHEMA_VERSION, SessionStore, StoreError};

impl SessionStore {
    pub fn create(path: &Path, session: NewSession) -> Result<OpenedSession> {
        ensure_empty_destination(path)?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let staging_root = parent.join(".staging");
        fs::create_dir_all(&staging_root).map_err(|source| io_error(&staging_root, source))?;
        let staging = staging_root.join(format!("session-{}", uuid::Uuid::new_v4()));

        let result = create_staged(&staging, path, parent, session);
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }
}

fn ensure_empty_destination(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(path).map_err(|source| io_error(path, source))?;
    if entries.next().is_some() {
        return Err(StoreError::InvalidEvent(format!(
            "session directory is not empty: {}",
            path.display()
        )));
    }
    Ok(())
}

fn create_staged(
    staging: &Path,
    destination: &Path,
    parent: &Path,
    session: NewSession,
) -> Result<OpenedSession> {
    prepare_staging(staging, &session)?;
    let opened = SessionStore::open_internal(staging, OpenOptions::default(), true)?;
    let event = RawEvent::new(
        uuid::Uuid::new_v4().to_string(),
        EventData::SessionCreated {
            session_id: session.session_id,
            cwd: session.cwd,
            root: session.root,
            created_at: session.created_at,
        },
    )?;
    opened.store.append(
        0,
        ProposedCommit::one(uuid::Uuid::new_v4().to_string(), session.created_at, event),
    )?;
    drop(opened);
    if destination.exists() {
        fs::remove_dir(destination).map_err(|source| io_error(destination, source))?;
    }
    fs::rename(staging, destination).map_err(|source| io_error(destination, source))?;
    sync_dir(staging.parent().unwrap_or(parent))?;
    sync_dir(parent)?;
    SessionStore::open(destination, OpenOptions::default())
}

fn prepare_staging(path: &Path, session: &NewSession) -> Result<()> {
    fs::create_dir(path).map_err(|source| io_error(path, source))?;
    fs::create_dir(path.join("events")).map_err(|source| io_error(path, source))?;
    fs::create_dir(path.join("snapshots")).map_err(|source| io_error(path, source))?;
    let identity = SessionIdentityFile {
        schema_version: SCHEMA_VERSION,
        session_id: session.session_id.clone(),
        cwd: session.cwd.clone(),
        created_at: session.created_at,
        journal_generation: uuid::Uuid::new_v4().to_string(),
        extensions: BTreeMap::new(),
    };
    atomic_json(&path.join("session.json"), &identity)?;
    create_open_segment(path, 1)?;
    sync_dir(path)
}
