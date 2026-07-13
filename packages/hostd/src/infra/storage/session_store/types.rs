//! Session store record types.
//!
//! The DTOs shared with `ports::session_store` (manifest, recovered agent,
//! committed message, ...) live in [`crate::ports::storage_types`] so
//! `application` can depend on them without importing `crate::infra`. This
//! module re-exports those names for existing `infra::storage` call sites
//! and keeps `AgentShardRecord`, the on-disk shard line format, local to the
//! adapter.

use serde::{Deserialize, Serialize};

pub use crate::ports::storage_types::{
    AgentExecutionManifestEntry, AgentManifestEntry, AgentShardHeader, CommittedMessage,
    RecoveredAgent, SESSION_SCHEMA_VERSION, SessionManifest,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub(super) enum AgentShardRecord {
    Header(AgentShardHeader),
    Message(CommittedMessage),
}
