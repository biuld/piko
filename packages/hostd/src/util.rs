//! Small crate-wide helpers shared by `protocol` and `application`.
//!
//! Kept intentionally tiny per `docs/ddd-layering.md` §9: "Do not require
//! hexagonal purity for tiny helpers (logging, `now_ms`)." Neither `protocol`
//! nor `application` may depend on the other, so shared leaf helpers live
//! here at the crate root instead.

use tokio::sync::mpsc::UnboundedSender;

use crate::api::{ProtocolError, ServerMessage};
use crate::infra::storage::SessionStorageError;

pub(crate) fn send_event(tx: &UnboundedSender<ServerMessage>, event: ServerMessage) {
    let _ = tx.send(event);
}

pub(crate) fn storage_error(error: SessionStorageError) -> ProtocolError {
    ProtocolError::InvalidCommand(error.to_string())
}

pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
