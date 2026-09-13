//! Public contract for the piko agent runtime.
//!
//! Integrators (such as hostd) depend on this crate for traits, errors, and
//! port types. The runtime implementation lives in the `orchd` crate.
//!
//! Product surface: [`AgentRuntimeApi`]. The short-lived ExecutionActor and
//! its request/receipt DTOs are orchd-internal (ADR-027): there are no
//! Execution-addressed public commands. Durable writes go through
//! [`ExecutionCommitPort`] and [`AgentCommitPort`].

pub mod agent;
pub mod approval;
pub mod error;
pub mod execution;
pub mod request;
pub mod response;
pub mod runtime_identity;
pub mod stream;
pub mod telemetry;
pub mod tools;

pub use agent::{
    AgentCommitPort, AgentInputRuntime, AgentRecoveryState, AgentRuntimeApi,
    RecoveredDetachedDelivery, RecoveredExecutionReport, SessionAgentConfig, SessionAgentHandle,
    SessionAgentPorts,
};
pub use approval::{
    ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest, is_approval_accepted,
};
pub use error::{AgentApiError, SessionStreamError, SnapshotRequiredReason};
pub use execution::{
    ApprovalPort, ExecutionCommitPort, InteractionPort, PromptAssemblyPort, RealtimeDeltaSink,
    SessionExecutionPorts, TrajectoryCapturePort,
};
pub use request::SubscribeRequest;
pub use response::SessionRuntimeSnapshot;
pub use runtime_identity::stable_internal_id;
pub use stream::{SessionOutputStream, SessionSubscription};
pub use tools::{
    ToolDiscoveryContext, ToolExecError, ToolExecResult, ToolExecutionContext, ToolProvider,
};

// Re-export durable work DTOs shared with hostd.
pub use piko_protocol::agent_work::{
    AgentInputDisposition, AgentWorkOutcome, CommitAck, CommitError,
};
pub use piko_protocol::{
    AgentActivity, AgentArtifactRef, AgentCommitAck, AgentDurableCommand, AgentInboxItem,
    AgentInboxSnapshot, AgentInput, AgentInputCancelReceipt, AgentInputDelivery,
    AgentInputDispositionChange, AgentInputReceipt, AgentInstanceId, AgentInstanceIdentity,
    AgentInstanceLifecycle, AgentInterruptReceipt, AgentLifecycleReceipt, AgentLifecycleRequest,
    AgentSnapshot, AgentSpecId, AgentWorkReport, ConsumeAgentInboxReceipt,
    ConsumeAgentInboxRequest, CreateAgentReceipt, CreateAgentRequest, SendAgentInputRequest,
};
