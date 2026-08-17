//! Outbound port for the durable AgentInstance session store.
//!
//! `application` reads/queries session storage only through
//! [`SessionStorePort`] (and opens/creates stores only through
//! [`SessionStoreFactory`]); the concrete filesystem-backed implementation
//! (`SessionStore`) lives in `adapters::storage`. Durable mutation still
//! flows through `piko_orchd_api::AgentCommitPort`, implemented by the same
//! adapter.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use super::storage_types::{
    AgentProjection, CommittedMessage, RawJournalEventRef, RecoveredAgent, SessionProjection,
    SessionStorageError,
};

/// Narrow async read/query surface used by the application. The filesystem
/// adapter keeps the underlying store synchronous and offloads whole calls.
#[async_trait]
pub trait SessionStorePort: Send + Sync {
    async fn load_projection(&self) -> Result<SessionProjection, SessionStorageError>;

    async fn load_agent(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<RecoveredAgent, SessionStorageError>;

    async fn agent_instances(&self) -> Result<Vec<AgentProjection>, SessionStorageError>;

    async fn find_committed_message(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Result<Option<CommittedMessage>, SessionStorageError>;

    async fn agent_report_for_turn(
        &self,
        turn_id: &str,
    ) -> Result<Option<piko_protocol::AgentRunReport>, SessionStorageError>;

    async fn interrupt_incomplete_agent_executions(&self) -> Result<usize, SessionStorageError>;

    /// All raw journal events in commit order, including optional
    /// (`ignorable`) event types (F-36 trajectory replay).
    async fn raw_journal_events(&self) -> Result<Vec<RawJournalEventRef>, SessionStorageError>;

    /// Current journal revision. Does not clone the aggregate.
    async fn journal_revision(&self) -> Result<u64, SessionStorageError>;

    /// Raw events with `revision > after_revision`.
    async fn raw_journal_events_after(
        &self,
        after_revision: u64,
    ) -> Result<Vec<RawJournalEventRef>, SessionStorageError>;
}

/// Opens or creates a [`SessionStorePort`] for a given session directory.
#[async_trait]
pub trait SessionStoreFactory: Send + Sync {
    fn open(&self, session_dir: &Path) -> Arc<dyn SessionStorePort>;

    async fn create(
        &self,
        session_dir: &Path,
        session_id: String,
        cwd: String,
        created_at: i64,
    ) -> Result<Arc<dyn SessionStorePort>, SessionStorageError>;

    async fn delete(&self, session_dir: &Path) -> Result<(), SessionStorageError>;
}
