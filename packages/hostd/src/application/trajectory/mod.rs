//! Read-only trajectory query (F-36).
//!
//! Replays the session journal's raw events — acknowledged facts plus
//! optional `trajectory.*` records — and joins them by run identity into
//! run summaries and full run records for the web viewer. Queries never
//! mutate session state or invoke the model gateway.
//!
//! The first read for a session decodes the journal; later list/fetch calls
//! reuse that projection and apply only newer revisions.

mod decode;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use piko_protocol::{TrajectoryRun, TrajectoryRunListPage};
use tokio::sync::Mutex;

use crate::api::ProtocolError;
use crate::ports::session_repository::SessionRepositoryPort;
use crate::ports::session_store::{SessionStoreFactory, SessionStorePort};
use crate::util::LruMap;

use self::decode::{DecodedSession, apply_events, summarize};

const DEFAULT_RUN_LIMIT: usize = 50;
const MAX_RUN_LIMIT: usize = 200;
const CURSOR_PREFIX: &str = "run:";
/// Upper bound on decoded session caches (each pins a journal handle and its
/// in-memory replay).
const TRAJECTORY_CACHE_CAPACITY: usize = 64;

#[derive(Clone)]
pub struct TrajectoryQuery {
    pub(crate) session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
    pub(crate) store_factory: Arc<dyn SessionStoreFactory>,
    pub(crate) storage: Option<Arc<dyn SessionRepositoryPort>>,
    cache: Arc<Mutex<LruMap<String, CachedSession>>>,
}

struct CachedSession {
    store: Arc<dyn SessionStorePort>,
    decoded: DecodedSession,
}

impl TrajectoryQuery {
    pub fn new(
        session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
        store_factory: Arc<dyn SessionStoreFactory>,
        storage: Option<Arc<dyn SessionRepositoryPort>>,
    ) -> Self {
        Self {
            session_paths,
            store_factory,
            storage,
            cache: Arc::new(Mutex::new(LruMap::new(TRAJECTORY_CACHE_CAPACITY))),
        }
    }

    async fn session_dir(&self, session_id: &str) -> Result<PathBuf, ProtocolError> {
        if let Some(path) = self.session_paths.lock().await.get(session_id).cloned() {
            return Ok(path);
        }
        // Resume-friendly fallback: resolve persisted sessions through the
        // repository even when this hostd process has not opened them.
        // Summaries avoid reconstructing full HostState for every session.
        if let Some(storage) = &self.storage {
            let all = storage
                .summaries(None)
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
            if let Some(path) = all.into_iter().find_map(|summary| {
                (summary.session_id == session_id).then_some(summary.session_path)?
            }) {
                return Ok(PathBuf::from(path));
            }
        }
        Err(ProtocolError::InvalidCommand(format!(
            "trajectory unavailable for session {session_id}"
        )))
    }

    async fn decoded_session(&self, session_id: &str) -> Result<DecodedSession, ProtocolError> {
        let mut cache = self.cache.lock().await;
        let entry = if cache.get(session_id).is_some() {
            cache.get_mut(session_id).expect("cache entry just checked")
        } else {
            let session_dir = self.session_dir(session_id).await?;
            let store = self.store_factory.open(&session_dir);
            cache.insert(
                session_id.to_string(),
                CachedSession {
                    store,
                    decoded: DecodedSession::default(),
                },
            );
            cache
                .get_mut(session_id)
                .expect("cache entry just inserted")
        };
        let store = entry.store.clone();
        let revision = store
            .journal_revision()
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        if entry.decoded.revision == revision {
            return Ok(entry.decoded.clone());
        }
        let events = if entry.decoded.revision == 0 {
            store
                .raw_journal_events()
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?
        } else {
            store
                .raw_journal_events_after(entry.decoded.revision)
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?
        };
        apply_events(&mut entry.decoded, &events);
        entry.decoded.revision = entry.decoded.revision.max(revision);
        Ok(entry.decoded.clone())
    }

    /// List runs, newest first, bounded and cursor-paged.
    pub async fn list_runs(
        &self,
        session_id: &str,
        agent_instance_id: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
        dropped: &HashMap<String, u32>,
    ) -> Result<TrajectoryRunListPage, ProtocolError> {
        let decoded = self.decoded_session(session_id).await?;
        let limit = if limit == 0 { DEFAULT_RUN_LIMIT } else { limit }.min(MAX_RUN_LIMIT);
        let mut runs = decoded
            .runs
            .into_iter()
            .filter(|(_, run)| {
                agent_instance_id
                    .is_none_or(|agent| run.agent_instance_id.as_deref() == Some(agent))
            })
            .map(|(run_id, run)| {
                let summary = summarize(session_id, &run_id, &run, dropped);
                (run_id, summary)
            })
            .collect::<Vec<_>>();
        runs.sort_by(|(left_id, left), (right_id, right)| {
            right
                .started_at
                .cmp(&left.started_at)
                .then_with(|| right_id.cmp(left_id))
        });
        let after = cursor.and_then(|value| value.strip_prefix(CURSOR_PREFIX));
        let mut iter = runs.into_iter().peekable();
        if let Some(after_run) = after {
            while iter
                .peek()
                .is_some_and(|(run_id, _)| run_id.as_str() != after_run)
            {
                iter.next();
            }
            iter.next(); // resume after the cursor run
        }
        let mut page = Vec::new();
        for (_, summary) in iter.by_ref() {
            if page.len() >= limit {
                break;
            }
            page.push(summary);
        }
        let next_cursor = iter
            .peek()
            .map(|(run_id, _)| format!("{CURSOR_PREFIX}{run_id}"));
        Ok(TrajectoryRunListPage {
            runs: page,
            next_cursor,
        })
    }

    /// Fetch one full run record.
    pub async fn fetch_run(
        &self,
        session_id: &str,
        run_id: &str,
        dropped: &HashMap<String, u32>,
    ) -> Result<TrajectoryRun, ProtocolError> {
        let decoded = self.decoded_session(session_id).await?;
        let (run_id, run) = decoded
            .runs
            .into_iter()
            .find(|(candidate, _)| candidate == run_id)
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(format!("trajectory run {run_id} not found"))
            })?;
        Ok(TrajectoryRun {
            summary: summarize(session_id, &run_id, &run, dropped),
            assembly: run.assembly,
            records: run.records,
            messages: run.messages,
        })
    }
}

#[cfg(test)]
mod tests;
