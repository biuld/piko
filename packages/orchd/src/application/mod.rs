// ---- Application: Supervisor — multi-agent lifecycle manager ----

pub mod supervisor;
pub mod agent_spawner;
pub mod run;
pub mod bootstrap;
pub mod snapshot;
mod utils;

pub use supervisor::{PendingStream, Supervisor};
