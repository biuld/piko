use super::*;

impl AgentActor {
    pub(super) async fn handle_input(
        &mut self,
        request: SendAgentInputRequest,
        input_id: Option<String>,
        submitted_at: Option<i64>,
    ) -> Result<AcceptedAgentInput, AgentApiError> {
        if let Some((existing, accepted)) = self.input_requests.get(&request.request_id) {
            if existing != &request
                || input_id
                    .as_deref()
                    .is_some_and(|input_id| accepted.receipt.input_id != input_id)
            {
                return Err(AgentApiError::IdempotencyConflict);
            }
            let mut duplicate = accepted.clone();
            duplicate.receipt.disposition = InputDisposition::Duplicate;
            return Ok(duplicate);
        }
        let execution_id = internal_execution_id(&self.identity, &request.request_id);
        if self.completed_executions.contains_key(&execution_id) {
            return Ok(AcceptedAgentInput {
                receipt: AgentInputReceipt {
                    input_id: input_id
                        .clone()
                        .unwrap_or_else(|| request.request_id.clone()),
                    request_id: request.request_id,
                    session_id: self.identity.session_id.clone(),
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    disposition: InputDisposition::Duplicate,
                    run_id: None,
                    queued_position: None,
                },
                internal_execution_id: execution_id,
            });
        }
        let result = self
            .handle_input_once(request.clone(), input_id, submitted_at)
            .await;
        if let Ok(accepted) = &result {
            self.input_requests
                .insert(request.request_id.clone(), (request, accepted.clone()));
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
        input_id: Option<String>,
        submitted_at: Option<i64>,
    ) -> Result<AcceptedAgentInput, AgentApiError> {
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
            ) => {
                let execution_id = internal_execution_id(&self.identity, &request.request_id);
                self.start_execution(request, input_id, submitted_at)
                    .await
                    .map(|receipt| AcceptedAgentInput {
                        receipt,
                        internal_execution_id: execution_id,
                    })
            }
            (Some(_), AgentInputDelivery::StartWhenIdle) => {
                Err(AgentApiError::ExecutionAlreadyActive)
            }
            (Some(_), AgentInputDelivery::FollowUp) => {
                let execution_id = internal_execution_id(&self.identity, &request.request_id);
                self.enqueue_follow_up(
                    request,
                    input_id,
                    submitted_at,
                    None,
                    tracing::Span::current(),
                )
                .await
                .map_err(|(error, _)| error)
                .map(|receipt| AcceptedAgentInput {
                    receipt,
                    internal_execution_id: execution_id,
                })
            }
            (Some(execution_id), AgentInputDelivery::Auto | AgentInputDelivery::SteerActive) => {
                let execution_id = execution_id.to_string();
                let (root_input_id, active_run_id) = self
                    .input_requests
                    .values()
                    .find(|(_, accepted)| accepted.internal_execution_id == execution_id)
                    .map(|(request, accepted)| {
                        (
                            accepted.receipt.input_id.clone(),
                            accepted
                                .receipt
                                .run_id
                                .clone()
                                .unwrap_or_else(|| request.request_id.clone()),
                        )
                    })
                    .ok_or(AgentApiError::InvalidState)?;
                let input_id = input_id.unwrap_or_else(|| request.request_id.clone());
                let submitted_at =
                    submitted_at.unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                let mut canonical_input =
                    piko_protocol::AgentInput::from_request(&request, submitted_at);
                canonical_input.input_id = input_id.clone();
                self.commit
                    .commit_agent_command(
                        &self.identity.session_id,
                        AgentDurableCommand::AgentInputAdmitted {
                            admission: piko_protocol::AgentInputAdmission {
                                input: canonical_input,
                                disposition: InputDisposition::PendingSteer,
                                root_input_id: Some(root_input_id.clone()),
                                run_id: None,
                                bound_run_id: Some(active_run_id.clone()),
                                admitted_at: submitted_at,
                            },
                        },
                    )
                    .await
                    .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
                let receipt = self
                    .execution
                    .steer_execution(SteerExecutionRequest {
                        request_id: request.request_id.clone(),
                        input_id: Some(input_id.clone()),
                        session_id: self.identity.session_id.clone(),
                        execution_id: execution_id.clone(),
                        message_id: request.message_id.clone(),
                        content: request.content.clone(),
                        submitted_at,
                    })
                    .await;
                let receipt = match receipt {
                    Ok(receipt) => receipt,
                    // Admission is already durable. Keep the input pending
                    // when the live mailbox cannot accept it so a retry can
                    // re-deliver the same canonical proposal. Cancelling here
                    // would make a later idempotent admission unable to retry
                    // delivery.
                    Err(error) => return Err(error),
                };
                Ok(AcceptedAgentInput {
                    receipt: AgentInputReceipt {
                        input_id,
                        request_id: receipt.request_id,
                        session_id: receipt.session_id,
                        agent_instance_id: self.identity.agent_instance_id.clone(),
                        disposition: receipt.disposition,
                        run_id: None,
                        queued_position: None,
                    },
                    internal_execution_id: execution_id,
                })
            }
        }
    }
}
