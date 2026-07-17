//! [`SessionStorePort`] / [`SessionStoreFactory`] implementation wrapping
//! the filesystem-backed [`SessionStore`].

use std::path::Path;
use std::sync::Arc;

use crate::infra::storage::SessionStore;
use crate::ports::session_store::{SessionStoreFactory, SessionStorePort};
use crate::ports::storage_types::{
    AgentManifestEntry, CommittedMessage, RecoveredAgent, SessionManifest, SessionStorageError,
};

impl SessionStorePort for SessionStore {
    fn load_manifest(&self) -> Result<SessionManifest, SessionStorageError> {
        SessionStore::load_manifest(self)
    }

    fn load_agent(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Result<RecoveredAgent, SessionStorageError> {
        SessionStore::load_agent(self, session_id, agent_instance_id)
    }

    fn agent_instances(&self) -> Result<Vec<AgentManifestEntry>, SessionStorageError> {
        SessionStore::agent_instances(self)
    }

    fn find_committed_message(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Result<Option<CommittedMessage>, SessionStorageError> {
        SessionStore::find_committed_message(self, session_id, agent_instance_id, message_id)
    }

    fn find_committed_message_lenient(
        &self,
        agent_instance_id: &str,
        message_id: &str,
    ) -> Option<CommittedMessage> {
        SessionStore::find_committed_message_lenient(self, agent_instance_id, message_id)
    }

    fn agent_report_for_turn(
        &self,
        turn_id: &str,
    ) -> Result<Option<piko_protocol::AgentRunReport>, SessionStorageError> {
        SessionStore::agent_report_for_turn(self, turn_id)
    }

    fn interrupt_incomplete_agent_executions(&self) -> Result<usize, SessionStorageError> {
        SessionStore::interrupt_incomplete_agent_executions(self)
    }
}

/// Default [`SessionStoreFactory`] backed by the real filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsSessionStoreFactory;

impl SessionStoreFactory for FsSessionStoreFactory {
    fn open(&self, session_dir: &Path) -> Arc<dyn SessionStorePort> {
        Arc::new(SessionStore::new(session_dir))
    }

    fn create(
        &self,
        session_dir: &Path,
        session_id: String,
        cwd: String,
        created_at: i64,
    ) -> Result<Arc<dyn SessionStorePort>, SessionStorageError> {
        let store = SessionStore::create_session(session_dir, session_id, cwd, created_at)?;
        Ok(Arc::new(store))
    }
}
