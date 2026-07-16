//! Outbound port for the durable AgentInstance session store.
//!
//! `application` reads/queries session storage only through
//! [`SessionStorePort`] (and opens/creates stores only through
//! [`SessionStoreFactory`]); the concrete filesystem-backed implementation
//! (`SessionStore`) lives in `adapters::storage`. Durable mutation still
//! flows through `orchd_api::AgentCommitPort`, implemented by the same
//! adapter.

use std::path::Path;
use std::sync::Arc;

use super::storage_types::{
    AgentManifestEntry, CommittedMessage, RecoveredAgent, SessionManifest, SessionStorageError,
};

/// Narrow, synchronous read/query surface of the durable session store that
/// `application` actually calls.
pub trait SessionStorePort: Send + Sync {
    fn load_manifest(&self) -> Result<SessionManifest, SessionStorageError>;

    fn load_agent(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<RecoveredAgent, SessionStorageError>;

    fn agent_instances(&self) -> Result<Vec<AgentManifestEntry>, SessionStorageError>;

    fn find_committed_message(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Result<Option<CommittedMessage>, SessionStorageError>;

    /// Scan an agent shard for a message without requiring the full shard to
    /// parse cleanly. Used by the observation path when concurrent appends
    /// may leave a trailing partial line.
    fn find_committed_message_lenient(
        &self,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Option<CommittedMessage>;

    fn agent_report_for_turn(
        &self,
        turn_id: &str,
    ) -> Result<Option<piko_protocol::AgentRunReport>, SessionStorageError>;

    fn interrupt_incomplete_agent_executions(&self) -> Result<usize, SessionStorageError>;
}

/// Opens or creates a [`SessionStorePort`] for a given session directory.
pub trait SessionStoreFactory: Send + Sync {
    fn open(&self, session_dir: &Path) -> Arc<dyn SessionStorePort>;

    fn create(
        &self,
        session_dir: &Path,
        session_id: String,
        cwd: String,
        created_at: i64,
    ) -> Result<Arc<dyn SessionStorePort>, SessionStorageError>;
}
