//! Host adapter for the schema-v4 append-only session journal.

mod journal;
mod serial;

pub use crate::ports::storage_types::{
    AgentProjection, CommittedMessage, RecoveredAgent, RootInputProjection, SessionProjection,
};
pub use journal::SessionStore;
pub(crate) use journal::mutations::tree_entry_event;
