//! Outbound and inbound ports owned by hostd.
//!
//! Application and protocol depend on these traits. Adapters implement them.

pub mod prompt_materials;
pub mod session_repository;
pub mod session_store;
pub mod storage_types;
pub mod trajectory_registry;
pub mod transcript_estimator;
pub mod turn_runner;

pub use prompt_materials::PromptMaterialLoader;
pub use session_repository::SessionRepositoryPort;
pub use session_store::{SessionStoreFactory, SessionStorePort};
pub use storage_types::{
    AgentProjection, CommittedMessage, ExecutionProjection, PersistedSession, RecoveredAgent,
    SessionProjection, SessionStorageError,
};
pub use trajectory_registry::{NoopTrajectoryRegistry, TrajectoryRegistryPort};
pub use transcript_estimator::TranscriptEstimator;
pub use turn_runner::{
    AgentRunCompletion, AgentRunFailure, AgentRunRunner, AgentWorkAddress, ErrorAgentRunRunner,
    OperationRunCompletion, ResumeAgent, TurnEventStream,
};
