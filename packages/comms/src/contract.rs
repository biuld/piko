use std::collections::HashSet;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommunicationKind {
    DirectCall,
    Mailbox,
    Reply,
    LatestState,
    Observation,
    ClientOutput,
    ThreadBridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommunicationScope {
    Process,
    Connection,
    Session,
    Agent,
    Execution,
    Operation,
    Request,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryGuarantee {
    InMemory,
    ReliableObservation,
    BestEffort,
    LatestOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CapacityPolicy {
    Bounded(usize),
    One,
    Latest,
    Unbounded { justification: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OverflowPolicy {
    AwaitCapacity,
    RejectOverload,
    DropNewest,
    RecoverSnapshot,
    ReplaceLatest,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureMeaning {
    RuntimeUnavailable,
    ClientDisconnected,
    RequestCancelled,
    ObservationEnded,
    ProcessExited,
    NoSubscribers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationMeaning {
    ExplicitCommand,
    DropInterestOnly,
    DisconnectOnly,
    ScopeShutdown,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationSpec {
    pub id: &'static str,
    pub kind: CommunicationKind,
    pub owner: &'static str,
    pub producers: &'static [&'static str],
    pub consumer: &'static str,
    pub scope: CommunicationScope,
    pub delivery: DeliveryGuarantee,
    pub capacity: CapacityPolicy,
    pub overflow: OverflowPolicy,
    pub closure: ClosureMeaning,
    pub cancellation: CancellationMeaning,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("duplicate communication id: {0}")]
    DuplicateId(&'static str),
    #[error("communication {0} has no owner")]
    MissingOwner(&'static str),
    #[error("communication {0} has no consumer")]
    MissingConsumer(&'static str),
    #[error("communication {0} has an empty producer list")]
    MissingProducer(&'static str),
    #[error("communication {0} uses an invalid unbounded policy")]
    InvalidUnbounded(&'static str),
    #[error("communication {0} has an invalid mailbox policy")]
    InvalidMailbox(&'static str),
    #[error("communication {0} has an invalid reply policy")]
    InvalidReply(&'static str),
    #[error("communication {0} has an invalid latest-state policy")]
    InvalidLatest(&'static str),
    #[error("communication {0} drops reliable delivery")]
    ReliableDrop(&'static str),
    #[error("communication {0} exposes globally scoped product observation")]
    GlobalObservation(&'static str),
}

pub fn validate_catalog(specs: &[CommunicationSpec]) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();
    let mut ids = HashSet::new();
    for spec in specs {
        if !ids.insert(spec.id) {
            errors.push(ValidationError::DuplicateId(spec.id));
        }
        if spec.owner.is_empty() {
            errors.push(ValidationError::MissingOwner(spec.id));
        }
        if spec.consumer.is_empty() {
            errors.push(ValidationError::MissingConsumer(spec.id));
        }
        if spec.producers.is_empty() {
            errors.push(ValidationError::MissingProducer(spec.id));
        }
        if let CapacityPolicy::Unbounded { justification } = spec.capacity
            && (spec.kind != CommunicationKind::ThreadBridge || justification.is_empty())
        {
            errors.push(ValidationError::InvalidUnbounded(spec.id));
        }
        if matches!(
            spec.kind,
            CommunicationKind::Mailbox | CommunicationKind::ClientOutput
        ) && (!matches!(spec.capacity, CapacityPolicy::Bounded(capacity) if capacity > 0)
            || !matches!(
                spec.overflow,
                OverflowPolicy::AwaitCapacity | OverflowPolicy::RejectOverload
            ))
        {
            errors.push(ValidationError::InvalidMailbox(spec.id));
        }
        if spec.kind == CommunicationKind::Reply
            && (spec.capacity != CapacityPolicy::One || spec.scope != CommunicationScope::Request)
        {
            errors.push(ValidationError::InvalidReply(spec.id));
        }
        if spec.kind == CommunicationKind::LatestState
            && (spec.capacity != CapacityPolicy::Latest
                || spec.delivery != DeliveryGuarantee::LatestOnly
                || spec.overflow != OverflowPolicy::ReplaceLatest)
        {
            errors.push(ValidationError::InvalidLatest(spec.id));
        }
        if matches!(spec.delivery, DeliveryGuarantee::ReliableObservation)
            && matches!(spec.overflow, OverflowPolicy::DropNewest)
        {
            errors.push(ValidationError::ReliableDrop(spec.id));
        }
        if spec.kind == CommunicationKind::Observation && spec.scope == CommunicationScope::Process
        {
            errors.push(ValidationError::GlobalObservation(spec.id));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
