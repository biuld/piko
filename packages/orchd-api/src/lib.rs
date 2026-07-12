//! Public contract for the piko agent runtime.
//!
//! Integrators (such as hostd) depend on this crate for traits, errors, and
//! port types. The runtime implementation lives in the `orchd` crate.
//!
//! Product surface: [`AgentRuntimeApi`]. The lower-level [`AgentExecutor`]
//! contract exists for orchd's internal ExecutionActor implementation and
//! focused tests. Durable writes go through [`ExecutionCommitPort`] and
//! [`AgentCommitPort`]; there is no separate legacy Task/Work persistence
//! surface.

pub mod agent;
pub mod approval;
pub mod error;
pub mod execution;
pub mod request;
pub mod response;
pub mod stream;
pub mod tools;

pub use agent::{
    AgentCommitPort, AgentRecoveryState, AgentRuntimeApi, SessionAgentConfig, SessionAgentHandle,
    SessionAgentPorts,
};
pub use approval::{
    ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest, is_approval_accepted,
};
pub use error::{AgentApiError, SessionStreamError, SnapshotRequiredReason};
pub use execution::{
    AgentExecutor, ApprovalPort, ExecutionCommitPort, InteractionPort, RealtimeDeltaSink,
    SessionExecutionConfig, SessionExecutionHandle, SessionExecutionPorts,
};
pub use request::SubscribeRequest;
pub use response::{SessionRuntimeSnapshot, TaskSnapshot};
pub use stream::{SessionOutputStream, SessionSubscription};
pub use tools::{
    ToolDiscoveryContext, ToolExecError, ToolExecResult, ToolExecutionContext, ToolProvider,
};

// Re-export Execution DTOs used by the new API surface.
pub use piko_protocol::execution::{
    CancelExecutionRequest, CancelReason, CancelReceipt, CommitAck, CommitError,
    ConversationContext, ExecutionConfig, ExecutionId, ExecutionInputReceipt, ExecutionOutcome,
    ExecutionOutcomeCommit, ExecutionReceipt, ExecutionSnapshot, ExecutionStatus, InputDisposition,
    MessageCommit as ExecutionMessageCommit, StartExecutionRequest, SteerExecutionRequest,
};
pub use piko_protocol::{
    AgentActivity, AgentArtifactRef, AgentCommitAck, AgentDurableCommand, AgentExecutionReport,
    AgentInboxItem, AgentInboxSnapshot, AgentInputDelivery, AgentInputReceipt, AgentInstanceId,
    AgentInstanceIdentity, AgentInstanceLifecycle, AgentLifecycleReceipt, AgentLifecycleRequest,
    AgentSnapshot, AgentSpecId, ConsumeAgentInboxReceipt, ConsumeAgentInboxRequest,
    CreateAgentReceipt, CreateAgentRequest, SendAgentInputRequest, SteerAgentRequest,
};
