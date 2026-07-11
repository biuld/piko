// ---- Application: service facade and task supervision ----

pub mod agent_spawner;
pub mod bootstrap;
pub(crate) mod commands;
pub(crate) mod queries;
pub mod run;
pub mod service;
pub mod snapshot;
pub(crate) mod supervision;
mod utils;

pub use supervision::Supervisor;
