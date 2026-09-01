//! Schema-v4 session persistence.
//!
//! The journal is the sole durable authority. Query paths read durable
//! write-time projections under `readmodels/`.

mod accounting;
mod aggregate;
mod aggregate_queries;
mod aggregate_work;
mod aggregate_work_indexes;
mod aggregate_work_projection;
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
    ModelContinuity, StoredAgent, StoredAgentInput, StoredMessage, StoredModelStep,
    StoredRootProcessing, StoredTreeEntry,
};
pub use readmodels::{
    CatalogView, TrajectoryProjection, TrajectoryRunProjection, inspect_catalog, query_catalog,
    query_current, query_trajectory,
};
pub use schema::{
    AgentInputAdmittedV1, AgentInputAppliedV1, AgentInputDispositionChangedV1,
    AgentInputProcessingFinishedV1, AgentInputProcessingStartedV1, AgentInterruptRequestedV1,
    AgentPendingActionRequestedV1, AgentPendingActionResolvedV1, CompactionRecordedV1,
    Compatibility, EventData, MessageCommittedV1, ModelStepCommittedV1, RawEvent, SessionForkedV1,
    TreeEntryRecordedV1, UsageAttribution, UsageCorrectedV1, UsageRecordedV1,
};
pub const SCHEMA_VERSION: u32 = 4;
/// Event-decoder capability within schema-v4. It advances independently from
/// the on-disk storage generation.
pub const READER_VERSION: u32 = 3;
pub const COMMITS_PER_SEGMENT: u64 = 1_000;
