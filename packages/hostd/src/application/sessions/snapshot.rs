use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;

use super::helpers::{server_response_ok, session_reconciled_message};

impl HostApp {
    pub(crate) async fn apply_session_snapshot(
        &self,
        command_id: &str,
        session_id: String,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let (snapshot, agents) = self.session_view(&session_id).await?;
        Ok(vec![
            server_response_ok(command_id, crate::api::CommandResult::Empty),
            session_reconciled_message(
                session_id,
                piko_protocol::ReconcileReason::ExplicitRefresh,
                snapshot,
                agents,
            ),
        ])
    }
}
