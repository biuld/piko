use super::*;

#[async_trait]
impl AgentCommitPort for SessionStore {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let session_id = session_id.to_string();
        self.run_durable(move |store| store.commit_agent_command_unlocked(&session_id, command))
            .await
    }
}
