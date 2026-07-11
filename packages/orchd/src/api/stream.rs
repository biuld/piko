use std::pin::Pin;

use futures_core::Stream;
use piko_protocol::agent_runtime::{SessionCursor, SessionOutputEnvelope};

use super::SessionStreamError;

pub type SessionOutputStream =
    Pin<Box<dyn Stream<Item = Result<SessionOutputEnvelope, SessionStreamError>> + Send + 'static>>;

pub struct SessionSubscription {
    pub session_id: String,
    pub cursor: SessionCursor,
    pub output: SessionOutputStream,
}

impl std::fmt::Debug for SessionSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionSubscription")
            .field("session_id", &self.session_id)
            .field("cursor", &self.cursor)
            .finish_non_exhaustive()
    }
}
