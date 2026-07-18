//! Inbound messages to the Client Core reducer.

use piko_protocol::ServerMessage;

use crate::intent::ClientIntent;

/// Transport-level observations the frontend adapter reports to Core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportObservation {
    Connected,
    DecodeFailure { detail: String },
    SendFailure { detail: String },
    Closed,
}

/// Messages applied by [`crate::update::update`].
#[derive(Debug, Clone)]
pub enum ClientMsg {
    Intent(ClientIntent),
    Host(Box<ServerMessage>),
    Transport(TransportObservation),
}
