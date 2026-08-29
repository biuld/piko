use crate::api::ProtocolError;

use super::types::HostState;

impl HostState {
    pub fn apply_turn_input_disposition(
        &mut self,
        session_id: &str,
        turn_id: &str,
        disposition: piko_protocol::InputDisposition,
    ) -> Result<crate::api::TurnStatus, ProtocolError> {
        let state = self.session_mut(session_id)?;
        let turn = state
            .turns
            .get_mut(turn_id)
            .ok_or_else(|| ProtocolError::InvalidCommand(format!("turn not found: {turn_id}")))?;
        match disposition {
            piko_protocol::InputDisposition::Accepted
            | piko_protocol::InputDisposition::Duplicate => {
                turn.status = crate::api::TurnStatus::Running;
                state
                    .active_turns
                    .insert(turn.agent_instance_id.clone(), turn_id.to_string());
            }
            piko_protocol::InputDisposition::Queued => {
                turn.status = crate::api::TurnStatus::Queued;
            }
            piko_protocol::InputDisposition::Overload => {
                turn.status = crate::api::TurnStatus::Failed;
            }
            piko_protocol::InputDisposition::PendingFollowUp => {
                turn.status = crate::api::TurnStatus::Queued;
            }
            piko_protocol::InputDisposition::PendingSteer
            | piko_protocol::InputDisposition::AppliedAsRoot
            | piko_protocol::InputDisposition::AppliedToStep => {
                turn.status = crate::api::TurnStatus::Running;
                state
                    .active_turns
                    .insert(turn.agent_instance_id.clone(), turn_id.to_string());
            }
            piko_protocol::InputDisposition::Cancelled => {
                turn.status = crate::api::TurnStatus::Cancelled;
            }
        }
        Ok(turn.status)
    }
}
