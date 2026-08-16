use std::path::Path;

use crate::journal::{DurableCommit, SessionStore, VerificationReport};
use crate::replay::read_all;
use crate::{Result, SessionAggregate, StoreError};

/// One raw journal event with its durable commit position. Observational
/// readers (for example the trajectory query) use this to replay optional
/// event types without affecting the acknowledged session projection.
#[derive(Debug, Clone, PartialEq)]
pub struct RawJournalEvent {
    pub revision: u64,
    pub committed_at: i64,
    pub event: crate::RawEvent,
}

impl SessionStore {
    pub fn verify(&self) -> Result<VerificationReport> {
        let (commits, _, segments) = read_all(&self.inner.path, false)?;
        Ok(VerificationReport {
            revision: commits.last().map_or(0, |commit| commit.revision),
            segment_count: segments,
        })
    }

    pub fn aggregate(&self) -> SessionAggregate {
        self.inner
            .aggregate
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub(crate) fn path(&self) -> &Path {
        &self.inner.path
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.inner.session_id
    }

    pub(crate) fn journal_generation(&self) -> &str {
        &self.inner.journal_generation
    }

    pub(crate) fn commit_at(&self, revision: u64) -> Result<DurableCommit> {
        read_all(&self.inner.path, false)?
            .0
            .into_iter()
            .find(|commit| commit.revision == revision)
            .ok_or_else(|| StoreError::InvalidEvent(format!("missing commit revision {revision}")))
    }

    /// All raw journal events in commit order, including optional
    /// (`ignorable`) event types that the acknowledged projection skips.
    pub fn raw_events(&self) -> Result<Vec<RawJournalEvent>> {
        let (commits, _, _) = read_all(&self.inner.path, false)?;
        let mut events = Vec::new();
        for commit in commits {
            for event in commit.events {
                events.push(RawJournalEvent {
                    revision: commit.revision,
                    committed_at: commit.committed_at,
                    event,
                });
            }
        }
        Ok(events)
    }
}
