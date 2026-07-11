// ---- Application: Supervisor — multi-agent lifecycle manager ----

pub mod agent_spawner;
pub mod bootstrap;
pub mod commands;
pub mod run;
pub mod service;
pub mod snapshot;
pub(crate) mod supervision;
pub mod supervisor;
pub(crate) mod task_driver;
pub(crate) mod task_launcher;
pub(crate) mod task_registry;
mod utils;

pub use supervisor::Supervisor;
