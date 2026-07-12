//! Schema-v3 agent-oriented session storage.
//!
//! Each durable `AgentInstance` owns exactly one append-only shard under
//! `agents/<agent_instance_id>.jsonl`. `session.json` is a rebuildable
//! manifest and never contains transcript messages.
//!
//! Unlike the legacy schema-v2 `TaskRepository`, there is no Task/Work
//! lifecycle projection and no per-execution shard: a single AgentInstance
//! shard accumulates its whole conversation across Turns and Executions.

use std::path::PathBuf;

mod commit;
mod create;
mod io;
mod manifest;
mod reads;
mod types;

pub use types::{
    AgentManifestEntry, AgentShardHeader, CommittedMessage, RecoveredAgent, SESSION_SCHEMA_VERSION,
    SessionManifest,
};

#[derive(Debug, Clone)]
pub struct SessionStore {
    session_dir: PathBuf,
}

impl SessionStore {
    pub fn new(session_dir: impl Into<PathBuf>) -> Self {
        Self {
            session_dir: session_dir.into(),
        }
    }

    pub(super) fn manifest_path(&self) -> PathBuf {
        self.session_dir.join("session.json")
    }

    pub(super) fn agents_dir(&self) -> PathBuf {
        self.session_dir.join("agents")
    }

    pub(super) fn agent_path(&self, agent_instance_id: &str) -> PathBuf {
        self.agents_dir().join(format!("{agent_instance_id}.jsonl"))
    }
}
