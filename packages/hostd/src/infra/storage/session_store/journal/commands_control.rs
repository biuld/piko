use piko_protocol::{AgentDurableCommand, AgentInputDisposition, CommitError};

use super::SessionStore;

impl SessionStore {
    pub async fn cancel_pending_agent_input(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_protocol::AgentInputCancelReceipt, CommitError> {
        let session_id = session_id.to_string();
        let agent_instance_id = agent_instance_id.to_string();
        let input_id = input_id.to_string();
        self.run_durable(move |store| {
            let aggregate = store.aggregate().map_err(Self::commit_error)?;
            if aggregate.session_id.as_deref() != Some(session_id.as_str()) {
                return Err(CommitError::IdentityMismatch);
            }
            let Some(stored) = aggregate.agent_inputs.get(&input_id) else {
                return Ok(piko_protocol::AgentInputCancelReceipt {
                    input_id: input_id.clone(),
                    request_id: input_id,
                    session_id,
                    agent_instance_id,
                    accepted: false,
                });
            };
            if stored.input.agent_instance_id != agent_instance_id {
                return Err(CommitError::IdentityMismatch);
            }
            let request_id = stored.input.request_id.clone();
            if stored.disposition != AgentInputDisposition::PendingFollowUp {
                return Ok(piko_protocol::AgentInputCancelReceipt {
                    input_id,
                    request_id,
                    session_id,
                    agent_instance_id,
                    accepted: false,
                });
            }
            store.commit_agent_command_unlocked(
                &session_id,
                AgentDurableCommand::AgentInputDispositionChanged {
                    change: piko_protocol::AgentInputDispositionChange {
                        agent_instance_id: agent_instance_id.clone(),
                        input_id: input_id.clone(),
                        disposition: AgentInputDisposition::Cancelled,
                        root_input_id: None,
                        model_step_id: None,
                        changed_at: chrono::Utc::now().timestamp_millis(),
                    },
                },
            )?;
            Ok(piko_protocol::AgentInputCancelReceipt {
                input_id,
                request_id,
                session_id,
                agent_instance_id,
                accepted: true,
            })
        })
        .await
    }

    pub async fn request_agent_interrupt(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        requested_at: i64,
    ) -> Result<Option<String>, CommitError> {
        let session_id = session_id.to_string();
        let agent_instance_id = agent_instance_id.to_string();
        self.run_durable(move |store| {
            let aggregate = store.aggregate().map_err(Self::commit_error)?;
            if aggregate.session_id.as_deref() != Some(session_id.as_str()) {
                return Err(CommitError::IdentityMismatch);
            }
            let Some(root_input_id) = aggregate
                .active_root_by_agent
                .get(&agent_instance_id)
                .cloned()
            else {
                return Ok(None);
            };
            store.commit_agent_command_unlocked(
                &session_id,
                AgentDurableCommand::InterruptRequested {
                    agent_instance_id,
                    root_input_id: root_input_id.clone(),
                    requested_at,
                },
            )?;
            Ok(Some(root_input_id))
        })
        .await
    }
}
