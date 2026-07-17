//! Typed communication contracts and policy-enforcing channel wrappers.
//!
//! This crate is a generic runtime leaf. Domain crates select a declared
//! contract and payload type; they cannot construct an unclassified channel
//! through this API.

#![allow(clippy::disallowed_methods)]

mod catalog;
mod contract;
mod topology;
mod wrappers;

pub use catalog::{ALL_SPECS, contracts};
pub use contract::{
    CancellationMeaning, CapacityPolicy, ClosureMeaning, CommunicationKind, CommunicationScope,
    CommunicationSpec, DeliveryGuarantee, OverflowPolicy, ValidationError, validate_catalog,
};
pub use topology::{render_json, render_mermaid};
pub use wrappers::{
    BroadcastReceiver, BroadcastSender, LatestReceiver, LatestSender, MailboxReceiver,
    MailboxSender, ReplyReceiver, ReplySender, ThreadBridgeReceiver, ThreadBridgeSender, broadcast,
    latest, mailbox, reply, thread_bridge,
};

/// A marker for a catalog entry that may construct a bounded MPSC mailbox.
pub trait MailboxContract: Send + Sync + 'static {
    const SPEC: &'static CommunicationSpec;
}

/// A marker for a catalog entry that may construct a single-value reply.
pub trait ReplyContract: Send + Sync + 'static {
    const SPEC: &'static CommunicationSpec;
}

/// A marker for a catalog entry that may construct a latest-state watch.
pub trait LatestContract: Send + Sync + 'static {
    const SPEC: &'static CommunicationSpec;
}

/// A marker for a catalog entry that may construct a broadcast observation lane.
pub trait BroadcastContract: Send + Sync + 'static {
    const SPEC: &'static CommunicationSpec;
}

/// A marker for an explicitly justified sync/async thread bridge.
pub trait ThreadBridgeContract: Send + Sync + 'static {
    const SPEC: &'static CommunicationSpec;
}
