// orchd — piko orchestrator daemon library

#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod adapters;
pub mod api;
pub mod application;
pub mod domain;
pub mod ports;
pub mod protocol;
pub mod runtime;

// Re-export key types
pub use api::{AgentApiError, AgentRuntime, SessionOutputStream, SessionSubscription};
pub use application::Supervisor;
pub use application::service::AgentRuntimeService;
pub use ports::agent_spawner::AgentReport;

pub mod integration {
    pub use crate::ports::persist_sink::{
        MessageCommit, PersistAck, PersistError, PersistSink, TaskEventCommit,
    };
}
