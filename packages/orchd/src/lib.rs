// orchd — piko orchestrator daemon library

#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(dead_code)]

mod adapters;
mod application;
mod domain;
mod ports;
mod runtime;

pub mod api;
pub mod host;
#[doc(hidden)]
pub mod testing;

pub mod integration {
    pub use crate::ports::persist_sink::{
        MessageCommit, PersistAck, PersistError, PersistSink, TaskEventCommit, WorkEventCommit,
    };
}

pub use api::{
    AgentApiError, AgentRuntime, AgentRuntimeService, SessionOutputStream, SessionSubscription,
};
