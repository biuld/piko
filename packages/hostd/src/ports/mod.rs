//! Outbound and inbound ports owned by hostd.
//!
//! Application and protocol depend on these traits. Adapters implement them.

pub mod agent_runner;
pub mod prompt_materials;
pub mod session_repository;
pub mod session_store;
pub mod storage_types;
pub mod trajectory_registry;
pub mod transcript_estimator;

pub use agent_runner::{
    AgentRunCompletion, AgentRunEventStream, AgentRunFailure, AgentRunRunner, AgentWorkAddress,
    ErrorAgentRunRunner, OperationRunCompletion, ResumeAgent,
};
pub use prompt_materials::PromptMaterialLoader;
pub use session_repository::SessionRepositoryPort;
pub use session_store::{SessionStoreFactory, SessionStorePort};
pub use storage_types::{
    AgentProjection, CommittedMessage, PersistedSession, RecoveredAgent, RootInputProjection,
    SessionProjection, SessionStorageError,
};
pub use trajectory_registry::{NoopTrajectoryRegistry, TrajectoryRegistryPort};
pub use transcript_estimator::TranscriptEstimator;
