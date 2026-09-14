//! Read-only, journal-derived session trajectory queries (F-52 / D-69).

mod detail;
mod mapping;
mod stream;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use piko_protocol::{HistoryLaneSummary, HistoryStreamPage, SessionHistoryOverview};
use tokio::sync::Mutex;

use crate::api::{HistoryItemDetail, HistoryItemRef, ProtocolError};
use crate::ports::session_repository::SessionRepositoryPort;
use crate::ports::session_store::SessionStoreFactory;

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 400;
/// Lane strips stay compact; older blocks remain inspectable via the stream.
const LANE_BLOCK_LIMIT: usize = 512;

/// Read-model bundle cached by session directory, validated against the
/// requested published revision before reuse.
pub(crate) type InspectionCache =
    Arc<Mutex<HashMap<String, (u64, Arc<piko_session_store::InspectionBundle>)>>>;

#[derive(Clone)]
pub struct SessionHistoryQuery {
    session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
    store_factory: Arc<dyn SessionStoreFactory>,
    storage: Option<Arc<dyn SessionRepositoryPort>>,
    cache: InspectionCache,
}

impl SessionHistoryQuery {
    pub fn new(
        session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
        store_factory: Arc<dyn SessionStoreFactory>,
        storage: Option<Arc<dyn SessionRepositoryPort>>,
        cache: InspectionCache,
    ) -> Self {
        Self {
            session_paths,
            store_factory,
            storage,
            cache,
        }
    }

    /// Load the aligned inspection bundle. When the caller pins an expected
    /// revision, a cache entry at that revision is reused without reopening
    /// the store; otherwise the published snapshot is loaded fresh.
    async fn bundle(
        &self,
        session_id: &str,
        expected: Option<u64>,
    ) -> Result<Arc<piko_session_store::InspectionBundle>, ProtocolError> {
        let session_dir = self.session_dir(session_id).await?;
        let key = session_dir.to_string_lossy().to_string();
        if let Some(expected) = expected {
            let cache = self.cache.lock().await;
            if let Some((revision, bundle)) = cache.get(&key)
                && *revision == expected
            {
                return Ok(Arc::clone(bundle));
            }
        }
        let bundle = Arc::new(
            self.store_factory
                .open(&session_dir)
                .inspection()
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?,
        );
        let mut cache = self.cache.lock().await;
        cache.insert(key, (bundle.revision, Arc::clone(&bundle)));
        Ok(bundle)
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
    ) -> Result<SessionHistoryOverview, ProtocolError> {
        let bundle = self.bundle(session_id, None).await?;
        Ok(mapping::overview(session_id, &bundle))
    }

    pub async fn agent_stream(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        expected_revision: u64,
        after_cursor: Option<&str>,
        limit: Option<u32>,
    ) -> Result<HistoryStreamPage, ProtocolError> {
        let bundle = self.bundle(session_id, Some(expected_revision)).await?;
        require_revision(expected_revision, bundle.revision)?;
        if !bundle.current.agents.contains_key(agent_instance_id) {
            return Err(ProtocolError::InvalidCommand(format!(
                "history agent {agent_instance_id} not found"
            )));
        }
        let limit = page_limit(limit);
        let offset = cursor_offset(
            after_cursor,
            &format!("agent:{agent_instance_id}"),
            bundle.revision,
        )?;
        Ok(stream::agent_stream(
            session_id,
            agent_instance_id,
            &bundle,
            offset,
            limit,
        ))
    }

    pub async fn lane_summary(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        expected_revision: u64,
    ) -> Result<HistoryLaneSummary, ProtocolError> {
        let bundle = self.bundle(session_id, Some(expected_revision)).await?;
        require_revision(expected_revision, bundle.revision)?;
        if !bundle.current.agents.contains_key(agent_instance_id) {
            return Err(ProtocolError::InvalidCommand(format!(
                "history agent {agent_instance_id} not found"
            )));
        }
        Ok(stream::lane_summary(session_id, agent_instance_id, &bundle))
    }

    pub async fn item_detail(
        &self,
        session_id: &str,
        item_ref: &HistoryItemRef,
    ) -> Result<HistoryItemDetail, ProtocolError> {
        let bundle = self.bundle(session_id, Some(item_ref.revision)).await?;
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

fn require_revision(expected: u64, actual: u64) -> Result<(), ProtocolError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ProtocolError::HistoryRevisionChanged {
            current_revision: actual,
        })
    }
}
