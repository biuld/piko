pub mod jsonl_io;
pub mod jsonl_repository;
pub mod recovery;
pub mod task_repository;
pub mod types;

pub use jsonl_io::{append_jsonl, write_header};
pub use recovery::{
    agent_task_state_from_recovered, transcript_entries_from_recovered,
    transcript_messages_from_entries, transcript_messages_from_recovered,
    transcript_messages_from_session_entries,
};
pub use task_repository::{
    CommittedMessage, RecoveredTask, SessionManifest, TaskRepository, TaskShardHeader,
};
pub use types::JsonlSessionRepository;
pub use types::{PersistedSession, SessionStorageConfig, SessionStorageError};
