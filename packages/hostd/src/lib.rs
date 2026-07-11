pub mod api;
pub mod domain;
pub mod infra;
pub mod protocol;

// Re-export public API for external consumers (tests, main.rs)
pub use domain::sessions::{HostState, SessionState};
pub use protocol::{HostServer, run_stdio_server};
