//! Application layer: use-case services orchestrating `domain` policy through
//! `ports` (implemented by `adapters`).
//!
//! Dependency rule: `application` must not
//! `use crate::protocol`. `protocol` depends on `application`, never the
//! other way around.

mod agent_work_control;
pub mod compaction;
mod guardian;
pub mod host_app;
mod observability;
mod observation;
pub mod sessions;
mod trajectory;
pub mod turns;

pub(crate) use agent_work_control::AgentWorkControl;
pub use host_app::HostApp;
pub use trajectory::TrajectoryQuery;
