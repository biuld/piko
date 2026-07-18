//! Effects produced by the Client Core reducer.

use piko_protocol::Command;

/// Side effects the frontend adapter must execute.
#[derive(Debug, Clone)]
pub enum ClientEffect {
    Send(Command),
}
