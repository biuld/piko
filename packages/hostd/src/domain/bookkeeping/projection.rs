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

    /// Pair a terminal turn lifecycle event with a usage projection when applicable.
    pub fn with_usage_projection(
        &self,
        terminal: ServerMessage,
        size: Option<u64>,
    ) -> Vec<ServerMessage> {
        let projection = match &terminal {
            ServerMessage::TurnLifecycle(crate::api::TurnEvent::Completed {
                session_id,
                turn_id,
                agent_instance_id,
                usage,
                ..
            })
            | ServerMessage::TurnLifecycle(crate::api::TurnEvent::Failed {
                session_id,
                turn_id,
                agent_instance_id,
                usage,
                ..
            })
            | ServerMessage::TurnLifecycle(crate::api::TurnEvent::Cancelled {
                session_id,
                turn_id,
                agent_instance_id,
                usage,
                ..
            }) => self
                .usage_updated_event(
                    session_id,
                    Some(agent_instance_id.clone()),
                    Some(turn_id.clone()),
                    Some(usage),
                    size,
                )
                .ok(),
            _ => None,
        };
        match projection {
            Some(usage_event) => vec![terminal, usage_event],
            None => vec![terminal],
        }
    }
}
