pub mod api;
pub mod domain;
pub mod infra;
pub mod protocol;

// Re-export public API for external consumers (tests, main.rs)
pub use domain::sessions::{HostState, SessionState};
pub use protocol::{HostServer, run_stdio_server};

// Backward-compatible re-exports for tests that use old paths
pub mod state {
    pub use crate::domain::sessions::{HostState, QueueUpdateEvent, SessionState};
}

pub mod server {
    pub use crate::protocol::{HostServer, run_jsonl_server, run_stdio_server};
}

pub mod turn_runner {
    pub use crate::domain::turns::runner::*;
}

pub mod turn {
    pub mod runner {
        pub use crate::domain::turns::runner::*;
    }
}

pub mod session {
    pub use crate::infra::storage::{
        JsonlSessionRepository, PersistedSession, SessionStorageConfig, SessionStorageError,
    };
}

pub mod settings {
    pub use crate::domain::config::{
        CompactionSettings, HostSettings, McpServerConfig, SandboxSettings, SettingsManager,
    };
}

pub mod models {
    pub use crate::domain::config::ModelRegistry;
}

pub mod skills {
    pub use crate::domain::prompts::skills::*;
}

pub mod prompts {
    pub use crate::domain::prompts::*;
}

pub mod compaction {
    pub use crate::domain::compaction::*;
}

pub mod mcp {
    pub use crate::domain::config::McpServerConfig;
    pub use crate::infra::mcp::*;
}
