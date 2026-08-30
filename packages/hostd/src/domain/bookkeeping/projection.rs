//! Host-authored usage chrome projection (F-22 / D-34 / F-32).

use crate::api::{ProtocolError, ServerMessage};
use crate::domain::sessions::HostState;
use crate::util::now_ms;

impl HostState {
    /// Build a host-authoritative usage chrome projection.
    ///
    /// `size` is the resolved model context window when known. `used` is the
    /// last-step provider fill (`input + cache_read`), not occupancy.
    pub fn usage_updated_event(
        &self,
        session_id: &str,
        agent_instance_id: Option<String>,
        turn_id: Option<String>,
        turn_usage: Option<&crate::api::Usage>,
        size: Option<u64>,
    ) -> Result<ServerMessage, ProtocolError> {
        let session = self.session(session_id)?;
        Ok(ServerMessage::Usage(crate::api::UsageEvent::Updated {
            session_id: session_id.to_string(),
            agent_instance_id,
            turn_id,
            used: turn_usage.map(crate::api::Usage::context_fill).unwrap_or(0),
            size: size.filter(|value| *value > 0),
            cumulative: Some(session.cumulative_usage.clone()),
            turn_usage: turn_usage.cloned(),
            timestamp: now_ms(),
        }))
    }
}
