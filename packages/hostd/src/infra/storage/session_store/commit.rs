use async_trait::async_trait;
use orchd_api::AgentCommitPort;
use piko_protocol::execution::{CommitAck, CommitError, MessageCommit};
use piko_protocol::{
    AgentCommitAck, AgentDurableCommand, AgentInboxItem, AgentInstanceLifecycle, AgentRunReport,
};

use super::super::SessionStorageError;
use super::SessionStore;
use super::io::storage_commit_error;
use super::types::*;

impl SessionStore {
    pub fn interrupt_incomplete_agent_executions(&self) -> Result<usize, SessionStorageError> {
        self.with_io(|| self.interrupt_incomplete_agent_executions_unlocked())
    }

    fn interrupt_incomplete_agent_executions_unlocked(&self) -> Result<usize, SessionStorageError> {
        let mut manifest = self.load_manifest()?;
        let mut interrupted = 0;
        for execution in manifest.agent_executions.values_mut() {
            if !matches!(
                execution.status,
                piko_protocol::ExecutionStatus::Accepted | piko_protocol::ExecutionStatus::Running
            ) {
                continue;
            }
            let report = AgentRunReport {
                agent_instance_id: execution.agent_instance_id.clone(),
                report_id: interrupted_report_id(&execution.run_id),
                outcome: piko_protocol::ExecutionOutcome::Cancelled {
                    reason: Some("interrupted during session recovery".into()),
                },
                summary: "Execution interrupted during session recovery".into(),
                usage: Default::default(),
                artifacts: Vec::new(),
            };
            execution.status = piko_protocol::ExecutionStatus::Cancelled;
            execution.report = Some(report.clone());
            execution.finished_at = Some(chrono::Utc::now().timestamp_millis());
            if let Some(agent) = manifest.agents.get_mut(&execution.agent_instance_id) {
                agent.latest_report = Some(report);
            }
            interrupted += 1;
        }
        if interrupted > 0 {
            manifest.agent_revision = manifest.agent_revision.saturating_add(interrupted as u64);
            manifest.updated_at = chrono::Utc::now().timestamp_millis();
            self.store_manifest(&manifest)?;
        }
        Ok(interrupted)
    }

    fn commit_agent_command_unlocked(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let mut manifest = self.load_manifest().map_err(storage_commit_error)?;
        if manifest.session_id != session_id {
            return Err(CommitError::IdentityMismatch);
        }

        let agent_instance_id = match command {
            AgentDurableCommand::Create { identity, spec } => {
                if identity.session_id != session_id {
                    return Err(CommitError::IdentityMismatch);
                }
                if let Some(existing) = manifest.agents.get_mut(&identity.agent_instance_id) {
                    if existing.identity == identity {
                        if existing.spec.is_none() {
                            existing.spec = Some(spec);
                            manifest.agent_revision = manifest.agent_revision.saturating_add(1);
                            let revision = manifest.agent_revision;
                            self.store_manifest(&manifest)
                                .map_err(storage_commit_error)?;
                            return Ok(AgentCommitAck {
                                session_id: session_id.to_string(),
                                agent_instance_id: identity.agent_instance_id,
                                revision,
                            });
                        }
                        if existing.spec.as_ref() != Some(&spec) {
                            return Err(CommitError::IdempotencyConflict);
                        }
                        return Ok(AgentCommitAck {
                            session_id: session_id.to_string(),
                            agent_instance_id: identity.agent_instance_id,
                            revision: manifest.agent_revision,
                        });
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                match &identity.parent_agent_instance_id {
                    Some(parent) if !manifest.agents.contains_key(parent) => {
                        return Err(CommitError::IdentityMismatch);
                    }
                    None if manifest.root_agent_instance_id.is_some() => {
                        return Err(CommitError::IdempotencyConflict);
                    }
                    None => {
                        manifest.root_agent_instance_id = Some(identity.agent_instance_id.clone())
                    }
                    Some(_) => {}
                }
                let id = identity.agent_instance_id.clone();
                let now = chrono::Utc::now().timestamp_millis();
                manifest.agents.insert(
                    id.clone(),
                    AgentManifestEntry {
                        identity,
                        spec: Some(spec),
                        lifecycle: AgentInstanceLifecycle::Open,
                        latest_report: None,
                        created_at: now,
                        updated_at: now,
                    },
                );
                id
            }
            AgentDurableCommand::SetLifecycle {
                agent_instance_id,
                lifecycle,
            } => {
                let agent = manifest
                    .agents
                    .get_mut(&agent_instance_id)
                    .ok_or(CommitError::IdentityMismatch)?;
                agent.lifecycle = lifecycle;
                agent.updated_at = chrono::Utc::now().timestamp_millis();
                agent_instance_id
            }
            AgentDurableCommand::RunStarted {
                agent_instance_id,
                run_id,
                internal_execution_id,
                request_id,
                source_turn_id,
                detached_recipient_agent_instance_id,
                prompt_assembly_version,
                prompt_digest,
                started_at,
            } => {
                if !manifest.agents.contains_key(&agent_instance_id) {
                    return Err(CommitError::IdentityMismatch);
                }
                if let Some(existing) = manifest.agent_executions.get(&run_id) {
                    if existing.agent_instance_id == agent_instance_id
                        && existing.execution_id == internal_execution_id
                        && existing.request_id == request_id
                        && existing.source_turn_id == source_turn_id
                        && existing.detached_recipient_agent_instance_id
                            == detached_recipient_agent_instance_id
                        && existing.prompt_assembly_version == prompt_assembly_version
                        && existing.prompt_digest == prompt_digest
                    {
                        return Ok(AgentCommitAck {
                            session_id: session_id.to_string(),
                            agent_instance_id,
                            revision: manifest.agent_revision,
                        });
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                if manifest.agent_executions.values().any(|entry| {
                    entry.agent_instance_id == agent_instance_id
                        && matches!(
                            entry.status,
                            piko_protocol::ExecutionStatus::Accepted
                                | piko_protocol::ExecutionStatus::Running
                        )
                }) {
                    return Err(CommitError::IdempotencyConflict);
                }
                manifest.agent_executions.insert(
                    run_id.clone(),
                    AgentExecutionManifestEntry {
                        agent_instance_id: agent_instance_id.clone(),
                        run_id,
                        execution_id: internal_execution_id,
                        request_id,
                        source_turn_id,
                        detached_recipient_agent_instance_id,
                        detached_report_delivered: false,
                        prompt_assembly_version,
                        prompt_digest,
                        status: piko_protocol::ExecutionStatus::Accepted,
                        started_at,
                        finished_at: None,
                        report: None,
                    },
                );
                agent_instance_id
            }
            AgentDurableCommand::RunTerminal {
                run_id,
                report,
                finished_at,
            } => {
                if let Some(existing) = manifest.agent_executions.get(&run_id) {
                    if existing.report.as_ref() == Some(&report)
                        && existing.finished_at == Some(finished_at)
                    {
                        return Ok(AgentCommitAck {
                            session_id: session_id.to_string(),
                            agent_instance_id: report.agent_instance_id,
                            revision: manifest.agent_revision,
                        });
                    }
                    if existing.report.is_some() {
                        return Err(CommitError::IdempotencyConflict);
                    }
                }
                let source = manifest
                    .agents
                    .get_mut(&report.agent_instance_id)
                    .ok_or(CommitError::IdentityMismatch)?;
                source.latest_report = Some(report.clone());
                source.updated_at = chrono::Utc::now().timestamp_millis();
                if let Some(execution) = manifest.agent_executions.get_mut(&run_id) {
                    if execution.agent_instance_id != report.agent_instance_id {
                        return Err(CommitError::IdentityMismatch);
                    }
                    execution.status = report.outcome.status();
                    execution.report = Some(report.clone());
                    execution.finished_at = Some(finished_at);
                } else {
                    return Err(CommitError::IdentityMismatch);
                }
                report.agent_instance_id
            }
            AgentDurableCommand::InputQueued {
                agent_instance_id,
                queued_input,
            } => {
                if !manifest.agents.contains_key(&agent_instance_id)
                    || queued_input.request.agent_instance_id != agent_instance_id
                {
                    return Err(CommitError::IdentityMismatch);
                }
                if let Some(existing) = manifest
                    .agent_input_queue
                    .iter()
                    .find(|input| input.queued_input_id == queued_input.queued_input_id)
                {
                    if existing == &queued_input {
                        return Ok(AgentCommitAck {
                            session_id: session_id.to_string(),
                            agent_instance_id,
                            revision: manifest.agent_revision,
                        });
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                manifest.agent_input_queue.push(queued_input);
                agent_instance_id
            }
            AgentDurableCommand::QueuedInputStarted {
                agent_instance_id,
                queued_input_id,
                run_id,
                internal_execution_id,
                request_id,
                source_turn_id,
                detached_recipient_agent_instance_id,
                prompt_assembly_version,
                prompt_digest,
                started_at,
            } => {
                if let Some(existing) = manifest.agent_executions.get(&run_id) {
                    if existing.agent_instance_id == agent_instance_id
                        && existing.execution_id == internal_execution_id
                        && existing.request_id == request_id
                        && existing.source_turn_id == source_turn_id
                        && existing.detached_recipient_agent_instance_id
                            == detached_recipient_agent_instance_id
                        && existing.prompt_assembly_version == prompt_assembly_version
                        && existing.prompt_digest == prompt_digest
                    {
                        return Ok(AgentCommitAck {
                            session_id: session_id.to_string(),
                            agent_instance_id,
                            revision: manifest.agent_revision,
                        });
                    }
                    return Err(CommitError::IdempotencyConflict);
                }
                let queue_index = manifest
                    .agent_input_queue
                    .iter()
                    .position(|input| {
                        input.queued_input_id == queued_input_id
                            && input.request.agent_instance_id == agent_instance_id
                    })
                    .ok_or(CommitError::IdentityMismatch)?;
                if manifest.agent_executions.values().any(|entry| {
                    entry.agent_instance_id == agent_instance_id
                        && matches!(
                            entry.status,
                            piko_protocol::ExecutionStatus::Accepted
                                | piko_protocol::ExecutionStatus::Running
                        )
                }) {
                    return Err(CommitError::IdempotencyConflict);
                }
                manifest.agent_input_queue.remove(queue_index);
                manifest.agent_executions.insert(
                    run_id.clone(),
                    AgentExecutionManifestEntry {
                        agent_instance_id: agent_instance_id.clone(),
                        run_id,
                        execution_id: internal_execution_id,
                        request_id,
                        source_turn_id,
                        detached_recipient_agent_instance_id,
                        detached_report_delivered: false,
                        prompt_assembly_version,
                        prompt_digest,
                        status: piko_protocol::ExecutionStatus::Accepted,
                        started_at,
                        finished_at: None,
                        report: None,
                    },
                );
                agent_instance_id
            }
            AgentDurableCommand::CommitReport {
                recipient_agent_instance_id,
                report,
            } => {
                if !manifest.agents.contains_key(&recipient_agent_instance_id)
                    || !manifest.agents.contains_key(&report.agent_instance_id)
                {
                    return Err(CommitError::IdentityMismatch);
                }
                let report_id = report.report_id.clone();
                if !manifest
                    .agent_inbox
                    .iter()
                    .any(|item| item.report_id == report_id)
                {
                    manifest.agent_inbox.push(AgentInboxItem {
                        report_id,
                        recipient_agent_instance_id: recipient_agent_instance_id.clone(),
                        source_agent_instance_id: report.agent_instance_id.clone(),
                        report: report.clone(),
                        committed_at: chrono::Utc::now().timestamp_millis(),
                        consumed_at: None,
                    });
                }
                if let Some(source) = manifest.agents.get_mut(&report.agent_instance_id) {
                    source.latest_report = Some(report.clone());
                }
                if let Some(run) = manifest.agent_executions.values_mut().find(|run| {
                    run.report
                        .as_ref()
                        .is_some_and(|stored| stored.report_id == report.report_id)
                }) && run.detached_recipient_agent_instance_id.as_deref()
                    == Some(recipient_agent_instance_id.as_str())
                {
                    run.detached_report_delivered = true;
                }
                recipient_agent_instance_id
            }
            AgentDurableCommand::ConsumeInboxItem {
                agent_instance_id,
                report_id,
                consumed_at,
            } => {
                let item = manifest
                    .agent_inbox
                    .iter_mut()
                    .find(|item| {
                        item.report_id == report_id
                            && item.recipient_agent_instance_id == agent_instance_id
                    })
                    .ok_or(CommitError::IdentityMismatch)?;
                item.consumed_at = Some(consumed_at);
                agent_instance_id
            }
        };

        manifest.agent_revision = manifest.agent_revision.saturating_add(1);
        manifest.updated_at = chrono::Utc::now().timestamp_millis();
        let revision = manifest.agent_revision;
        self.store_manifest(&manifest)
            .map_err(storage_commit_error)?;
        Ok(AgentCommitAck {
            session_id: session_id.to_string(),
            agent_instance_id,
            revision,
        })
    }
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

fn interrupted_report_id(run_id: &str) -> String {
    orchd_api::stable_internal_id("report", &["interrupted", run_id])
}

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
