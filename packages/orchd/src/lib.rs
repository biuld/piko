// orchd — piko orchestrator daemon library
//!
//! Product path: [`AgentRuntime`] (Session → AgentInstance → Execution → Model Step → Tool).
//! `ExecutionActor` is an internal short-lived implementation detail.

#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(dead_code)]

mod adapters;
mod domain;
mod ports;
mod runtime;

pub mod api;
pub mod events {
    pub use crate::runtime::events::hub::{SessionOutputHub, merged_output_stream};
}
#[doc(hidden)]
pub mod testing;
pub mod tools;

pub use api::{AgentApiError, SessionOutputStream, SessionSubscription};
pub use orchd_api;
pub use runtime::AgentRuntime;
