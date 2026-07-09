// ---- Application: Supervisor — multi-agent lifecycle manager ----

pub mod agent_spawner;
pub mod bootstrap;
pub mod run;
pub mod snapshot;
pub mod supervisor;
mod task_driver;
mod task_launcher;
mod task_registry;
mod utils;

pub use supervisor::Supervisor;
