pub mod jsonl_io;
pub mod jsonl_repository;
pub mod recovery;
pub mod session_store;
pub mod types;

pub use jsonl_io::{append_jsonl, write_header};
pub use recovery::{
    agent_task_state_from_manifest_entry, agent_transcript_entries, transcript_messages_from_agent,
    transcript_messages_from_entries, transcript_messages_from_session_entries,
};
pub use session_store::{
    AgentManifestEntry, AgentShardHeader, CommittedMessage, RecoveredAgent, SessionManifest,
    SessionStore,
};
pub use types::JsonlSessionRepository;
pub use types::{PersistedSession, SessionStorageConfig, SessionStorageError};
