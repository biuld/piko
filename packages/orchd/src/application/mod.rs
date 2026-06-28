// ---- Application: use case layer ----
//
// The application layer orchestrates domain entities and ports
// to implement use cases: register agents, spawn tasks, manage tools.

pub mod agents;
pub mod orchestrator;
pub mod snapshots;
pub mod tasks;
pub mod tools;

pub use orchestrator::OrchCore;
