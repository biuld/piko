//! Outbound and inbound ports owned by hostd.
//!
//! Application and protocol depend on these traits. Adapters implement them.

pub mod prompt_materials;
pub mod session_repository;
pub mod session_store;
pub mod storage_types;
pub mod turn_runner;

pub use prompt_materials::PromptMaterialLoader;
pub use session_repository::SessionRepositoryPort;
pub use session_store::{SessionStoreFactory, SessionStorePort};
pub use storage_types::{
    AgentExecutionManifestEntry, AgentManifestEntry, AgentShardHeader, CommittedMessage,
    PersistedSession, RecoveredAgent, SESSION_SCHEMA_VERSION, SessionManifest, SessionStorageError,
};
pub use turn_runner::{
    AgentOperationAddress, AgentRunCompletion, AgentRunCompletionReceiver, AgentRunFailure,
    AgentRunHandle, AgentRunInput, AgentRunRunner, ErrorAgentRunRunner, OperationRunCompletion,
    ResumeAgent, TurnEventStream,
};
