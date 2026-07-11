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
pub mod bootstrap;
pub mod tools;
#[doc(hidden)]
pub mod testing;

pub use bootstrap::Runtime;
pub use orchd_api;
pub use api::{
    AgentApiError, AgentRuntime, AgentRuntimeService, SessionOutputStream, SessionSubscription,
};
