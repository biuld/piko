use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::util::now_ms;

use super::helpers::server_response_ok;

impl HostApp {
    pub(crate) async fn apply_session_snapshot(
        &self,
        command_id: &str,
        session_id: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let state = self.state.lock().await;
        let snapshot = state.snapshot(&session_id)?;
        Ok(vec![server_response_ok(
            command_id,
            crate::api::CommandResult::StateSnapshot {
                session_id,
                snapshot,
                timestamp: now_ms(),
            },
        )])
    }
}
