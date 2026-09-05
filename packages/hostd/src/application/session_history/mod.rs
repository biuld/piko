//! Read-only, journal-derived session history queries (F-52 / D-69).

mod detail;
mod mapping;
mod transcript;
mod work;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use piko_protocol::{
    HistoryJournalPage, HistoryProvenanceFilter, HistoryTranscriptPage, HistoryWorkPage,
    SessionHistoryOverview,
};
use tokio::sync::Mutex;

use crate::api::{HistoryItemDetail, HistoryItemRef, ProtocolError};
use crate::ports::session_repository::SessionRepositoryPort;
use crate::ports::session_store::SessionStoreFactory;

use self::mapping::{commit_summary, overview};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

#[derive(Clone)]
pub struct SessionHistoryQuery {
    session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
    store_factory: Arc<dyn SessionStoreFactory>,
    storage: Option<Arc<dyn SessionRepositoryPort>>,
}

impl SessionHistoryQuery {
    pub fn new(
        session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
        store_factory: Arc<dyn SessionStoreFactory>,
        storage: Option<Arc<dyn SessionRepositoryPort>>,
    ) -> Self {
        Self {
            session_paths,
            store_factory,
            storage,
        }
    }

    async fn bundle(
        &self,
        session_id: &str,
    ) -> Result<piko_session_store::InspectionBundle, ProtocolError> {
        let session_dir = self.session_dir(session_id).await?;
        self.store_factory
            .open(&session_dir)
            .inspection()
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))
    }

    async fn session_dir(&self, session_id: &str) -> Result<PathBuf, ProtocolError> {
        if let Some(path) = self.session_paths.lock().await.get(session_id).cloned() {
            return Ok(path);
        }
        if let Some(storage) = &self.storage
            && let Some(path) = storage
                .resolve_session_dir(None, session_id)
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?
        {
            return Ok(path);
        }
        Err(ProtocolError::InvalidCommand(format!(
            "history unavailable for session {session_id}"
        )))
    }

    pub async fn overview(
        &self,
        session_id: &str,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<SessionHistoryOverview, ProtocolError> {
        let bundle = self.bundle(session_id).await?;
        Ok(overview(
            session_id,
            &bundle,
            cursor_offset(cursor, "work", bundle.revision)?,
            page_limit(limit),
        ))
    }

    pub async fn work_page(
        &self,
        session_id: &str,
        root_input_id: &str,
        expected_revision: u64,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<HistoryWorkPage, ProtocolError> {
        let bundle = self.bundle(session_id).await?;
        require_revision(expected_revision, bundle.revision)?;
        if bundle
            .current
            .agent_inputs
            .get(root_input_id)
            .is_none_or(|stored| stored.root_input_id.as_deref() != Some(root_input_id))
        {
            return Err(ProtocolError::InvalidCommand(format!(
                "history work {root_input_id} not found"
            )));
        }
        let offset = cursor_offset(cursor, &format!("item:{root_input_id}"), bundle.revision)?;
        Ok(work::work_page(
            session_id,
            root_input_id,
            &bundle,
            offset,
            page_limit(limit),
        ))
    }

    pub async fn transcript_page(
        &self,
        session_id: &str,
        expected_revision: u64,
        cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<HistoryTranscriptPage, ProtocolError> {
        let bundle = self.bundle(session_id).await?;
        require_revision(expected_revision, bundle.revision)?;
        let offset = cursor_offset(cursor, "transcript", bundle.revision)?;
        Ok(transcript::transcript_page(
            session_id,
            &bundle,
            offset,
            page_limit(limit),
        ))
    }

    pub async fn journal_page(
        &self,
        session_id: &str,
        expected_revision: u64,
        cursor: Option<&str>,
        limit: Option<u32>,
        filter: HistoryProvenanceFilter,
    ) -> Result<HistoryJournalPage, ProtocolError> {
        let bundle = self.bundle(session_id).await?;
        require_revision(expected_revision, bundle.revision)?;
        let commits = bundle
            .history
            .commits
            .iter()
            .filter_map(|commit| commit_summary(commit, filter, bundle.revision, &bundle))
            .collect::<Vec<_>>();
        let prefix = format!("commit:{filter:?}");
        let offset = cursor_offset(cursor, &prefix, bundle.revision)?;
        let (commits, next_cursor) =
            page(commits, offset, page_limit(limit), &prefix, bundle.revision);
        Ok(HistoryJournalPage {
            session_id: session_id.to_string(),
            revision: bundle.revision,
            commits,
            next_cursor,
        })
    }

    pub async fn item_detail(
        &self,
        session_id: &str,
        item_ref: &HistoryItemRef,
    ) -> Result<HistoryItemDetail, ProtocolError> {
        let bundle = self.bundle(session_id).await?;
        require_revision(item_ref.revision, bundle.revision)?;
        detail::resolve(item_ref, &bundle)
    }
}

fn page_limit(limit: Option<u32>) -> usize {
    limit
        .unwrap_or(DEFAULT_LIMIT as u32)
        .max(1)
        .min(MAX_LIMIT as u32) as usize
}

fn cursor_offset(
    cursor: Option<&str>,
    prefix: &str,
    revision: u64,
) -> Result<usize, ProtocolError> {
    let Some(cursor) = cursor else { return Ok(0) };
    let invalid = || ProtocolError::InvalidCommand("invalid history cursor".into());
    if cursor.len() > 1024 {
        return Err(invalid());
    }
    let suffix = cursor
        .strip_prefix(&format!("{prefix}:"))
        .ok_or_else(invalid)?;
    let (snapshot, offset) = suffix.split_once(':').ok_or_else(invalid)?;
    let snapshot = snapshot.parse().map_err(|_| invalid())?;
    let offset = offset.parse().map_err(|_| invalid())?;
    require_revision(snapshot, revision)?;
    Ok(offset)
}

pub(super) fn page<T>(
    values: Vec<T>,
    offset: usize,
    limit: usize,
    prefix: &str,
    revision: u64,
) -> (Vec<T>, Option<String>) {
    let total = values.len();
    let values = values
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let next = offset.saturating_add(values.len());
    let cursor = (next < total).then(|| format!("{prefix}:{revision}:{next}"));
    (values, cursor)
}

fn require_revision(expected: u64, actual: u64) -> Result<(), ProtocolError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ProtocolError::HistoryRevisionChanged {
            current_revision: actual,
        })
    }
}
