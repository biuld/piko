//! Public contract for the piko agent runtime.
//!
//! Integrators (such as hostd) depend on this crate for traits, errors, and
//! port types. The runtime implementation lives in the `orchd` crate.

pub mod approval;
pub mod error;
pub mod input;
pub mod persist;
pub mod request;
pub mod response;
pub mod runtime;
pub mod stream;
pub mod tools;

pub use approval::{
    ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest, is_approval_accepted,
};
pub use error::{AgentApiError, SessionStreamError, SnapshotRequiredReason};
pub use input::build_user_input;
pub use persist::{
    MessageCommit, PersistAck, PersistError, PersistSink, TaskEventCommit, WorkEventCommit,
};
pub use request::{
    CreateTaskRequest, InputReceipt, SubmitTaskInput, SubscribeRequest, TaskControlRequest,
};
pub use response::{SessionRuntimeSnapshot, TaskHandle, TaskSnapshot};
pub use runtime::AgentRuntime;
pub use stream::{SessionOutputStream, SessionSubscription};
pub use tools::{
    ToolDiscoveryContext, ToolExecError, ToolExecResult, ToolExecutionContext, ToolProvider,
};
