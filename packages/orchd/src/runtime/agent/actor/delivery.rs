use super::*;

impl AgentActor {
    pub(super) async fn cancel_input(
        &mut self,
        input_id: String,
    ) -> Result<piko_protocol::AgentInputCancelReceipt, AgentApiError> {
        let Some(index) = self
            .follow_ups
            .iter()
            .position(|queued| queued.input.input_id == input_id)
        else {
            return Ok(piko_protocol::AgentInputCancelReceipt {
                input_id: input_id.clone(),
                request_id: input_id,
                session_id: self.identity.session_id.clone(),
                agent_instance_id: self.identity.agent_instance_id.clone(),
                accepted: false,
            });
        };
        let queued = &self.follow_ups[index];
        let request_id = queued.request.request_id.clone();
        self.commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::AgentInputDispositionChanged {
                    change: piko_protocol::AgentInputDispositionChange {
                        agent_instance_id: self.identity.agent_instance_id.clone(),
                        input_id: input_id.clone(),
                        disposition: piko_protocol::AgentInputDisposition::Cancelled,
                        root_input_id: None,
                        model_step_id: None,
                        changed_at: now_ms(),
                    },
                },
            )
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        self.follow_ups.remove(index);
        self.publish_snapshot();
        Ok(piko_protocol::AgentInputCancelReceipt {
            input_id,
            request_id,
            session_id: self.identity.session_id.clone(),
            agent_instance_id: self.identity.agent_instance_id.clone(),
            accepted: true,
        })
    }

    pub(super) async fn advance_next_follow_up(&mut self) {
        if self.lifecycle != AgentInstanceLifecycle::Open
            || !matches!(self.run_state, AgentRunState::Idle)
        {
            return;
        }
        if let Some(follow_up) = self.follow_ups.pop_front() {
            self.pending_run_parent = Some(follow_up.parent.clone());
            match self
                .start_execution_from(
                    follow_up.request.clone(),
                    follow_up.input.detached_recipient_agent_instance_id.clone(),
                    Some(follow_up.input.clone()),
                )
                .await
            {
                Ok(_) => {
                    if let Some(target) = follow_up.detached {
                        self.register_detached_report(follow_up.input.input_id.clone(), target)
                            .await
                    }
                }
                Err(_) => {
                    self.follow_ups.push_front(follow_up);
                    let command_tx = self.command_tx.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        let _ = command_tx.send(AgentCommand::RetryQueuedInput).await;
                    });
                }
            }
        }
    }

    pub(super) async fn deliver_report_or_retry(&self, mut delivery: DetachedDeliveryScope) {
        match delivery
            .commit(&self.commit, &self.identity.session_id)
            .await
        {
            DetachedDeliveryResult::Committed(item) => {
                let Some(scope) = self.scope.upgrade() else {
                    return;
                };
                let Some(recipient) = scope.agent(delivery.recipient_agent_instance_id()).await
                else {
                    return;
                };
                let _ = recipient
                    .command_tx
                    .send(AgentCommand::InboxReport { item })
                    .await;
            }
            DetachedDeliveryResult::Retry { delay_ms } => {
                let command_tx = self.command_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    let _ = command_tx
                        .send(AgentCommand::RetryDetachedReport { delivery })
                        .await;
                });
            }
            DetachedDeliveryResult::PermanentFailure => {}
        }
    }
}
