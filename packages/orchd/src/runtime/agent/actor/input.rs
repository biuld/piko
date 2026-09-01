use super::*;

impl AgentActor {
    pub(super) async fn handle_input(
        &mut self,
        proposed_input: piko_protocol::AgentInput,
    ) -> Result<AcceptedAgentInput, AgentApiError> {
        let request = proposed_input.to_request();
        self.handle_input_request(request, proposed_input).await
    }

    pub(super) async fn handle_input_request(
        &mut self,
        request: SendAgentInputRequest,
        proposed_input: piko_protocol::AgentInput,
    ) -> Result<AcceptedAgentInput, AgentApiError> {
        if let Some((_, existing_input, accepted)) = self.input_requests.get(&request.request_id) {
            if existing_input != &proposed_input {
                return Err(AgentApiError::IdempotencyConflict);
            }
            if let Some(accepted) = accepted {
                return Ok(accepted.clone());
            }
        }
        let root_input_id = proposed_input.input_id.clone();
        if self.completed_executions.contains_key(&root_input_id) {
            return Ok(AcceptedAgentInput {
                receipt: AgentInputReceipt {
                    input_id: proposed_input.input_id.clone(),
                    request_id: request.request_id,
                    session_id: self.identity.session_id.clone(),
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
                    queued_position: None,
                },
                root_input_id,
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

        let active_root_input_id = self.run_state.root_input_id().map(str::to_string);
        match (active_root_input_id.as_deref(), request.delivery) {
            (None, AgentInputDelivery::SteerActive) => Err(AgentApiError::InvalidState),
            (
                None,
                AgentInputDelivery::Auto
                | AgentInputDelivery::StartWhenIdle
                | AgentInputDelivery::FollowUp,
            ) => self
                .start_execution(request, canonical_input)
                .await
                .map(|receipt| {
                    let root_input_id = receipt.input_id.clone();
                    AcceptedAgentInput {
                        receipt,
                        root_input_id,
                    }
                }),
            (Some(_), AgentInputDelivery::StartWhenIdle) => {
                Err(AgentApiError::ExecutionAlreadyActive)
            }
            (Some(_), AgentInputDelivery::FollowUp) => self
                .enqueue_follow_up(
                    request,
                    Some(canonical_input),
                    None,
                    tracing::Span::current(),
                )
                .await
                .map_err(|(error, _)| error)
                .map(|receipt| {
                    let root_input_id = receipt.input_id.clone();
                    AcceptedAgentInput {
                        receipt,
                        root_input_id,
                    }
                }),
            (Some(active_root), AgentInputDelivery::Auto | AgentInputDelivery::SteerActive) => {
                let root_input_id = active_root.to_string();
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
                                admitted_at: submitted_at,
                            },
                        },
                    )
                    .await
                    .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
                let session_id = self.identity.session_id.clone();
                let live_delivery = SteerExecutionRequest {
                    request_id: request.request_id.clone(),
                    input_id: input_id.clone(),
                    session_id: session_id.clone(),
                    root_input_id: root_input_id.clone(),
                    message_id: request.message_id.clone(),
                    content: request.content.clone(),
                    submitted_at,
                };
                let execution = Arc::clone(&self.execution);
                tokio::spawn(async move {
                    // Admission is already durable. Live delivery may wait for
                    // the next model boundary; recovery replays the pending
                    // input rather than making this mailbox acknowledgement
                    // part of the acceptance contract.
                    let _ = execution.steer_execution(live_delivery).await;
                });
                Ok(AcceptedAgentInput {
                    receipt: AgentInputReceipt {
                        input_id,
                        request_id: request.request_id,
                        session_id,
                        agent_instance_id: self.identity.agent_instance_id.clone(),
                        disposition: piko_protocol::AgentInputDisposition::PendingSteer,
                        queued_position: None,
                    },
                    root_input_id,
                })
            }
        }
    }
}
