#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentApiError {
    #[error("task not found")]
    TaskNotFound,
    #[error("execution not found")]
    ExecutionNotFound,
    #[error("session not attached")]
    SessionNotAttached,
    #[error("session already attached")]
    SessionAlreadyAttached,
    #[error("session and task do not match")]
    SessionMismatch,
    #[error("task is closed")]
    TaskClosed,
    #[error("task is terminated")]
    TaskTerminated,
    #[error("invalid task state")]
    InvalidState,
    #[error("execution already active")]
    ExecutionAlreadyActive,
    #[error("duplicate request")]
    DuplicateRequest,
    #[error("idempotency key conflicts with a previous payload")]
    IdempotencyConflict,
    #[error("input rejected")]
    InputRejected,
    #[error("mailbox overload")]
    Overload,
    #[error("persistence unavailable")]
    PersistenceUnavailable,
    #[error("persistence failed: {0}")]
    PersistenceFailed(String),
    #[error("runtime unavailable")]
    RuntimeUnavailable,
    #[error("a fresh snapshot is required")]
    SnapshotRequired,
    #[error("operation cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionStreamError {
    #[error("a fresh snapshot is required: {reason}")]
    SnapshotRequired { reason: SnapshotRequiredReason },
    #[error("session closed")]
    SessionClosed,
    #[error("runtime unavailable")]
    RuntimeUnavailable,
    #[error("session stream failed: {message}")]
    Internal { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotRequiredReason {
    #[error("cursor epoch changed")]
    EpochChanged,
    #[error("cursor expired")]
    CursorExpired,
    #[error("cursor is unknown")]
    CursorUnknown,
}
