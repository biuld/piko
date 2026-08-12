use std::path::Path;

use crate::journal::{DurableCommit, SessionStore, VerificationReport};
use crate::replay::read_all;
use crate::{Result, SessionAggregate, StoreError};

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
}
