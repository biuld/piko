//! Schema-v4 session persistence.
//!
//! The journal is the sole durable authority. Query paths read durable
//! write-time projections under `readmodels/`.

mod accounting;
mod aggregate;
mod aggregate_queries;
mod error;
mod journal;
mod journal_create;
mod journal_io;
mod journal_queries;
mod projection;
mod readmodels;
mod replay;
mod schema;
mod segments;

pub use accounting::{AccountingProjection, EffectiveUsageFact, UsageQuery, UsageSummary};
pub use aggregate::SessionAggregate;
pub use error::{Result, StoreError};
pub use journal::{
    DurableCommit, NewSession, OpenOptions, OpenedSession, ProposedCommit, RecoveryReport,
    SessionDescriptor, SessionStore, VerificationReport,
};
pub use journal_queries::JournalFacts;
pub use projection::{
    ModelContinuity, StoredAgent, StoredExecution, StoredMessage, StoredTreeEntry,
};
pub use readmodels::{
    CatalogView, TrajectoryProjection, TrajectoryRunProjection, inspect_catalog, query_catalog,
    query_current, query_trajectory,
};
pub use schema::{
    CompactionRecordedV1, Compatibility, EventData, ExecutionStartedV1, MessageCommittedV1,
    RawEvent, SessionForkedV1, TreeEntryRecordedV1, UsageAttribution, UsageCorrectedV1,
    UsageRecordedV1,
};
pub const SCHEMA_VERSION: u32 = 4;
/// Event-decoder capability within schema-v4. It advances independently from
/// the on-disk storage generation.
pub const READER_VERSION: u32 = 1;
pub const COMMITS_PER_SEGMENT: u64 = 1_000;
