//! Read-only trajectory query (F-36).
//!
//! Read the published trajectory read model. Queries never mutate session
//! state, invoke the model gateway, or replay the journal on the ordinary
//! path.

mod decode;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use piko_protocol::{TrajectoryRun, TrajectoryRunListPage};
use tokio::sync::Mutex;

use crate::api::ProtocolError;
use crate::ports::session_repository::SessionRepositoryPort;
use crate::ports::session_store::SessionStoreFactory;

use self::decode::{DecodedSession, summarize};

const DEFAULT_RUN_LIMIT: usize = 50;
const MAX_RUN_LIMIT: usize = 200;
const CURSOR_PREFIX: &str = "run:";

#[derive(Clone)]
pub struct TrajectoryQuery {
    pub(crate) session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
    pub(crate) store_factory: Arc<dyn SessionStoreFactory>,
    pub(crate) storage: Option<Arc<dyn SessionRepositoryPort>>,
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
        }
    }

    async fn session_dir(&self, session_id: &str) -> Result<PathBuf, ProtocolError> {
        if let Some(path) = self.session_paths.lock().await.get(session_id).cloned() {
            return Ok(path);
        }
        // Resume-friendly fallback: resolve persisted sessions through the
        // repository even when this hostd process has not opened them.
        // Summaries avoid reconstructing full HostState for every session.
        if let Some(storage) = &self.storage
            && let Some(path) = storage
                .resolve_session_dir(None, session_id)
                .await
                .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?
        {
            return Ok(path);
        }
        Err(ProtocolError::InvalidCommand(format!(
            "trajectory unavailable for session {session_id}"
        )))
    }

    async fn decoded_session(&self, session_id: &str) -> Result<DecodedSession, ProtocolError> {
        let session_dir = self.session_dir(session_id).await?;
        let store = self.store_factory.open(&session_dir);
        let projection = store
            .trajectory()
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        Ok(DecodedSession {
            runs: projection.runs,
        })
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
