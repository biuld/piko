pub mod adapters;
pub mod api;
pub mod application;
pub mod domain;
pub mod infra;
pub mod logging;
pub mod ports;
pub mod protocol;
pub mod util;

// Re-export public API for external consumers (tests, main.rs)
pub use adapters::OrchAgentRunRunner;
pub use domain::sessions::{HostState, SessionState};
pub use ports::{AgentRunInput, AgentRunRunner, ErrorAgentRunRunner, ResumeAgent};
pub use protocol::{HostServer, run_stdio_server};
