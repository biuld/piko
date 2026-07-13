use super::*;

impl AgentActor {
    pub(super) async fn handle_input(
        &mut self,
        request: SendAgentInputRequest,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        if let Some((existing, receipt)) = self.input_requests.get(&request.request_id) {
            if existing != &request {
                return Err(AgentApiError::IdempotencyConflict);
            }
            let mut duplicate = receipt.clone();
            duplicate.disposition = InputDisposition::Duplicate;
            return Ok(duplicate);
        }
        if let Some(execution_id) = request.requested_execution_id.as_deref()
            && self.completed_executions.contains_key(execution_id)
        {
            return Ok(AgentInputReceipt {
                request_id: request.request_id,
                session_id: self.identity.session_id.clone(),
                agent_instance_id: self.identity.agent_instance_id.clone(),
                execution_id: Some(execution_id.to_string()),
                disposition: InputDisposition::Duplicate,
            });
        }
        let result = self.handle_input_once(request.clone()).await;
        if let Ok(receipt) = &result {
            self.input_requests
                .insert(request.request_id.clone(), (request, receipt.clone()));
        }
        result
    }

    pub(super) async fn consume_inbox(
        &mut self,
        request: piko_protocol::ConsumeAgentInboxRequest,
    ) -> Result<piko_protocol::ConsumeAgentInboxReceipt, AgentApiError> {
        let index = self
            .inbox
            .iter()
            .position(|item| item.report_id == request.report_id)
            .ok_or(AgentApiError::InvalidState)?;
        if self.inbox[index].consumed_at.is_some() {
            return Ok(piko_protocol::ConsumeAgentInboxReceipt {
                request_id: request.request_id,
                session_id: request.session_id,
                agent_instance_id: request.agent_instance_id,
                report_id: request.report_id,
                consumed: false,
            });
        }
        self.commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::ConsumeInboxItem {
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    report_id: request.report_id.clone(),
                    consumed_at: request.consumed_at,
                },
            )
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        self.inbox[index].consumed_at = Some(request.consumed_at);
        self.publish_snapshot();
        Ok(piko_protocol::ConsumeAgentInboxReceipt {
            request_id: request.request_id,
            session_id: request.session_id,
            agent_instance_id: request.agent_instance_id,
            report_id: request.report_id,
            consumed: true,
        })
    }

    async fn handle_input_once(
        &mut self,
        request: SendAgentInputRequest,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        if self.lifecycle == AgentInstanceLifecycle::Closed {
            return Err(AgentApiError::AgentClosed);
        }
        if matches!(
            self.lifecycle,
            AgentInstanceLifecycle::Terminated | AgentInstanceLifecycle::Unavailable
        ) {
            return Err(AgentApiError::AgentTerminated);
        }

        match (self.run_state.execution_id(), request.delivery) {
            (None, AgentInputDelivery::SteerActive) => Err(AgentApiError::InvalidState),
            (
                None,
                AgentInputDelivery::Auto
                | AgentInputDelivery::StartWhenIdle
                | AgentInputDelivery::FollowUp,
            ) => self.start_execution(request).await,
            (Some(_), AgentInputDelivery::StartWhenIdle) => {
                Err(AgentApiError::ExecutionAlreadyActive)
            }
            (Some(execution_id), AgentInputDelivery::FollowUp) => {
                let _ = execution_id;
                self.enqueue_follow_up(request, None)
                    .await
                    .map_err(|(error, _)| error)
            }
            (Some(execution_id), AgentInputDelivery::Auto | AgentInputDelivery::SteerActive) => {
                let execution_id = execution_id.to_string();
                let receipt = self
                    .execution
                    .steer_execution(SteerExecutionRequest {
                        request_id: request.request_id.clone(),
                        session_id: self.identity.session_id.clone(),
                        execution_id: execution_id.clone(),
                        message_id: request.message_id,
                        content: request.content,
                        submitted_at: chrono::Utc::now().timestamp_millis(),
                    })
                    .await?;
                Ok(AgentInputReceipt {
                    request_id: receipt.request_id,
                    session_id: receipt.session_id,
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    execution_id: Some(execution_id),
                    disposition: receipt.disposition,
                })
            }
        }
    }
}
