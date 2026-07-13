//! Public contract for the piko agent runtime.
//!
//! Integrators (such as hostd) depend on this crate for traits, errors, and
//! port types. The runtime implementation lives in the `orchd` crate.
//!
//! Product surface: [`AgentRuntimeApi`]. ExecutionActor is an orchd-internal
//! implementation detail. Durable writes go through [`ExecutionCommitPort`] and
//! [`AgentCommitPort`].

pub mod agent;
pub mod approval;
pub mod error;
pub mod execution;
pub mod request;
pub mod response;
pub mod runtime_identity;
pub mod stream;
pub mod tools;

pub use agent::{
    AgentCommitPort, AgentRecoveryState, AgentRuntimeApi, RecoveredDetachedDelivery,
    RecoveredExecutionReport, SessionAgentConfig, SessionAgentHandle, SessionAgentPorts,
};
pub use approval::{
    ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest, is_approval_accepted,
};
pub use error::{AgentApiError, SessionStreamError, SnapshotRequiredReason};
pub use execution::{
    ApprovalPort, ExecutionCommitPort, InteractionPort, RealtimeDeltaSink, SessionExecutionPorts,
};
pub use request::SubscribeRequest;
pub use response::SessionRuntimeSnapshot;
pub use runtime_identity::stable_internal_id;
pub use stream::{SessionOutputStream, SessionSubscription};
pub use tools::{
    ToolDiscoveryContext, ToolExecError, ToolExecResult, ToolExecutionContext, ToolProvider,
};

// Re-export Execution DTOs used by the new API surface.
pub use piko_protocol::execution::{
    CancelReceipt, CommitAck, CommitError, ExecutionId, ExecutionOutcome, ExecutionStatus,
    InputDisposition, MessageCommit as ExecutionMessageCommit,
};
pub use piko_protocol::{
    AgentActivity, AgentArtifactRef, AgentCommitAck, AgentDurableCommand, AgentInboxItem,
    AgentInboxSnapshot, AgentInputDelivery, AgentInputReceipt, AgentInstanceId,
    AgentInstanceIdentity, AgentInstanceLifecycle, AgentLifecycleReceipt, AgentLifecycleRequest,
    AgentRunReport, AgentSnapshot, AgentSpecId, ConsumeAgentInboxReceipt, ConsumeAgentInboxRequest,
    CreateAgentReceipt, CreateAgentRequest, SendAgentInputRequest, SteerAgentRequest,
};
