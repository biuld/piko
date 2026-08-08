use super::*;

impl SessionStore {
    /// Commit a Message onto the durable AgentInstance shard, auto-creating
    /// the shard header if this is the first write.
    pub fn commit_message(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
    ) -> Result<CommitAck, CommitError> {
        self.with_io(|| self.commit_message_under_lock(commit, agent_spec_id))
    }

    /// Message commit body; caller must already hold the session IO lock.
    pub(crate) fn commit_message_under_lock(
        &self,
        commit: MessageCommit,
        agent_spec_id: &str,
    ) -> Result<CommitAck, CommitError> {
        self.ensure_agent_shard_under_lock(
            &commit.session_id,
            &commit.agent_instance_id,
            agent_spec_id,
            commit.committed_at,
        )
        .map_err(storage_commit_error)?;

        let recovered = self
            .load_agent(&commit.session_id, &commit.agent_instance_id)
            .map_err(storage_commit_error)?;

        if let Some(existing) = recovered
            .transcript
            .iter()
            .find(|message| message.id == commit.message_id)
        {
            if existing.parent_id == commit.parent_message_id
                && existing.message == commit.message
                && existing.execution_id.as_deref() == Some(commit.execution_id.as_str())
            {
                if recovered.head_message_id.as_deref() == Some(commit.message_id.as_str()) {
                    self.advance_root_leaf_under_lock(
                        &commit.agent_instance_id,
                        &commit.message_id,
                        commit.committed_at,
                    )
                    .map_err(storage_commit_error)?;
                }
                return Ok(CommitAck {
                    session_id: commit.session_id,
                    execution_id: commit.execution_id,
                    agent_instance_id: commit.agent_instance_id,
                    message_id: Some(commit.message_id),
                    revision: existing.transcript_seq,
                });
            }
            return Err(CommitError::IdempotencyConflict);
        }

        if commit.parent_message_id != recovered.head_message_id {
            return Err(CommitError::IdentityMismatch);
        }

        let transcript_seq = recovered.last_transcript_seq.saturating_add(1);
        let entry = CommittedMessage {
            id: commit.message_id.clone(),
            parent_id: commit.parent_message_id.clone(),
            agent_instance_id: commit.agent_instance_id.clone(),
            agent_spec_id: agent_spec_id.to_string(),
            execution_id: Some(commit.execution_id.clone()),
            source_turn_id: commit.source_turn_id.clone(),
            transcript_seq,
            timestamp: commit.committed_at,
            message: commit.message.clone(),
        };
        self.append_record(&commit.agent_instance_id, &AgentShardRecord::Message(entry))
            .map_err(storage_commit_error)?;
        self.advance_root_leaf_under_lock(
            &commit.agent_instance_id,
            &commit.message_id,
            commit.committed_at,
        )
        .map_err(storage_commit_error)?;

        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision: transcript_seq,
        })
    }
}
