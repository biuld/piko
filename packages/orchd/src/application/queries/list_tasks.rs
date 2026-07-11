use piko_protocol::agent_runtime::SessionRuntimeSnapshot;

use crate::api::AgentApiError;

use super::super::supervision::Supervisor;
use super::subscribe_session::session_snapshot;

pub(crate) async fn list_tasks(
    supervisor: &Supervisor,
    session_id: String,
) -> Result<SessionRuntimeSnapshot, AgentApiError> {
    session_snapshot(supervisor, session_id).await
}
