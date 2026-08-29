use super::*;

impl AgentActor {
    pub(super) async fn handle_input(
        &mut self,
        request: SendAgentInputRequest,
        canonical_input: Option<piko_protocol::AgentInput>,
    ) -> Result<AcceptedAgentInput, AgentApiError> {
        let explicit_input = canonical_input.is_some();
        let proposed_input = if let Some(input) = canonical_input {
            input
        } else if let Some((_, existing, _)) = self.input_requests.get(&request.request_id) {
            existing.clone()
        } else {
            piko_protocol::AgentInput::from_request(&request, now_ms())
        };
        if let Some((existing_request, existing_input, accepted)) =
            self.input_requests.get(&request.request_id)
        {
            if existing_request != &request || (explicit_input && existing_input != &proposed_input)
            {
                return Err(AgentApiError::IdempotencyConflict);
            }
            if let Some(accepted) = accepted {
                let mut duplicate = accepted.clone();
                duplicate.receipt.disposition = InputDisposition::Duplicate;
                return Ok(duplicate);
            }
        }
        let execution_id = internal_execution_id(&self.identity, &request.request_id);
        if self.completed_executions.contains_key(&execution_id) {
            return Ok(AcceptedAgentInput {
                receipt: AgentInputReceipt {
                    input_id: proposed_input.input_id.clone(),
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
            .handle_input_once(request.clone(), proposed_input.clone())
            .await;
        self.input_requests.insert(
            request.request_id.clone(),
            (request, proposed_input, result.as_ref().ok().cloned()),
        );
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
        canonical_input: piko_protocol::AgentInput,
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
                self.start_execution(request, canonical_input)
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
                    Some(canonical_input),
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
                    .run_state
                    .active_root()
                    .map(|(input_id, run_id)| (input_id.to_string(), run_id.to_string()))
                    .ok_or(AgentApiError::InvalidState)?;
                let input_id = canonical_input.input_id.clone();
                let submitted_at = canonical_input.submitted_at;
                self.commit
                    .commit_agent_command(
                        &self.identity.session_id,
                        AgentDurableCommand::AgentInputAdmitted {
                            admission: piko_protocol::AgentInputAdmission {
                                input: canonical_input,
                                disposition: piko_protocol::AgentInputDisposition::PendingSteer,
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
                    // Do not cancel after durable admit; the caller retries
                    // delivery with the same cached proposal.
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
