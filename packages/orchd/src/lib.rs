// orchd — piko orchestrator daemon library
//!
//! Product path: [`AgentExecutionRuntime`] (Session → Execution → Model Step → Tool).
//! Classic Task/Work runtime has been removed.

#![allow(clippy::large_enum_variant)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(dead_code)]

mod adapters;
mod domain;
mod ports;
mod runtime;

pub mod api;
#[doc(hidden)]
pub mod testing;
pub mod tools;

pub use api::{AgentApiError, SessionOutputStream, SessionSubscription};
pub use orchd_api;
pub use runtime::AgentExecutionRuntime;
