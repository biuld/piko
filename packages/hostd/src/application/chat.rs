use crate::api::ProtocolError;
use crate::application::host_app::HostApp;
use crate::util::{ClientEventSender, storage_error};

impl HostApp {
    /// Resolve one target-oriented user submission through host authority.
    /// The client never decides whether the target is a root Turn or a direct
    /// child Agent run.
    pub(crate) async fn apply_chat_submit(
        &self,
        command_id: String,
        session_id: String,
        target_agent_instance_id: String,
        content: piko_protocol::MessageContent,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        let cwd = self.state.lock().await.session_cwd(&session_id)?;
        let session_dir = self.ensure_turn_session_dir(&session_id, &cwd).await?;
        let projection = self
            .session_store_factory
            .open(&session_dir)
            .load_projection()
            .await
            .map_err(storage_error)?;
        let target = projection
            .agents
            .get(&target_agent_instance_id)
            .ok_or_else(|| {
                ProtocolError::InvalidCommand(format!(
                    "agent instance not found: {target_agent_instance_id}"
                ))
            })?;

        if target.lifecycle != piko_protocol::AgentInstanceLifecycle::Open {
            return Err(ProtocolError::InvalidCommand(format!(
                "agent instance is not open: {target_agent_instance_id}"
            )));
        }
        super::turns::content::validate_user_content(&content)?;
        self.submit_chat(
            command_id,
            session_id,
            target_agent_instance_id,
            content,
            tx,
        )
        .await
    }
}
