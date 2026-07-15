use tokio::sync::mpsc::UnboundedSender;

use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::util::storage_error;

impl HostApp {
    /// Resolve one target-oriented user submission through host authority.
    /// The client never decides whether the target is a root Turn or a direct
    /// child Agent run.
    pub(crate) async fn apply_chat_submit(
        &self,
        command_id: String,
        session_id: String,
        target_agent_instance_id: String,
        text: String,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<(), ProtocolError> {
        let cwd = self.state.lock().await.session_cwd(&session_id)?;
        let session_dir = self.ensure_turn_session_dir(&session_id, &cwd).await?;
        let manifest = self
            .session_store_factory
            .open(&session_dir)
            .load_manifest()
            .map_err(storage_error)?;
        let target = manifest
            .agents
            .get(&target_agent_instance_id)
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(format!(
                    "agent instance not found: {target_agent_instance_id}"
                ))
            })?;

        if manifest.root_agent_instance_id.as_deref() == Some(target_agent_instance_id.as_str()) {
            self.submit_root_chat(command_id, session_id, text, tx)
                .await
        } else {
            if target.lifecycle != piko_protocol::AgentInstanceLifecycle::Open {
                return Err(ProtocolError::InvalidCommand(format!(
                    "agent instance is not open: {target_agent_instance_id}"
                )));
            }
            self.submit_direct_agent_chat(
                command_id,
                session_id,
                target_agent_instance_id,
                text,
                tx,
            )
            .await
        }
    }
}
