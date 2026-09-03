//! Outbound port for durable AgentInstance session queries and control facts.
//!
//! `application` accesses session storage only through
//! [`SessionStorePort`] (and opens/creates stores only through
//! [`SessionStoreFactory`]); the concrete filesystem-backed implementation
//! (`SessionStore`) lives in `adapters::storage`. Runtime-originated durable
//! mutation still flows through `piko_orchd_api::AgentCommitPort`; host control
//! uses the narrow atomic mutations declared below.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use super::storage_types::{
    AgentProjection, CommittedMessage, RecoveredAgent, SessionProjection, SessionStorageError,
};
use crate::api::SessionTreeEntry;

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

    async fn agent_report_for_input(
        &self,
        root_input_id: &str,
    ) -> Result<Option<piko_protocol::AgentWorkReport>, SessionStorageError>;

    async fn interrupt_incomplete_agent_work(&self) -> Result<usize, SessionStorageError>;

    /// Atomically cancel one still-pending follow-up by durable identity.
    async fn cancel_pending_agent_input(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_protocol::AgentInputCancelReceipt, piko_protocol::CommitError>;

    /// Atomically persist interrupt intent for the current unfinished root.
    /// Returns that root identity, or `None` when the agent is durably idle.
    async fn request_agent_interrupt(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        requested_at: i64,
    ) -> Result<Option<String>, piko_protocol::CommitError>;

    /// Persist the host-authoritative session-tree cursor.
    async fn select_branch(&self, target_id: Option<&str>) -> Result<(), SessionStorageError>;

    /// Persist a non-message session-tree entry and its cursor transition.
    async fn append_tree_entry(&self, entry: SessionTreeEntry) -> Result<(), SessionStorageError>;

    async fn trajectory(
        &self,
    ) -> Result<piko_session_store::TrajectoryProjection, SessionStorageError>;
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
