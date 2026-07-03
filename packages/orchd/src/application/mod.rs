// ---- Application: Supervisor — multi-agent lifecycle manager ----

pub mod agent_spawner;
pub mod bootstrap;
pub mod run;
pub mod snapshot;
pub mod supervisor;
mod task_events;
mod utils;

pub use supervisor::Supervisor;
