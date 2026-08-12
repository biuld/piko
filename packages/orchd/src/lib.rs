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
/// F-04 conservative transcript estimator. hostd bookkeeping consumes this
/// as the single occupancy formula (F-32).
pub mod transcript {
    pub use crate::domain::transcript::tokens::{estimate_messages, message_tokens, text_tokens};
}

pub use api::{AgentApiError, SessionOutputStream, SessionSubscription};
pub use piko_orchd_api;
pub use runtime::AgentRuntime;
