use super::*;

impl AgentActor {
    pub(super) fn register_waiter(
        &mut self,
        execution_id: String,
        reply: ReplySender<AgentRunReportContract, Result<AgentRunReport, AgentApiError>>,
    ) {
        if let Some(report) = self.completed_executions.get(&execution_id) {
            let _ = reply.send(Ok(report.clone()));
        } else if self.run_state.execution_id() == Some(&execution_id) {
            self.execution_waiters
                .entry(execution_id)
                .or_default()
                .push(reply);
        } else {
            let _ = reply.send(Err(AgentApiError::ExecutionNotFound));
        }
    }

    pub(super) async fn cancel_input(
        &mut self,
        request_id: String,
    ) -> Result<AgentCancelReceipt, AgentApiError> {
        let Some(index) = self
            .follow_ups
            .iter()
            .position(|input| input.durable.queued_input_id == request_id)
        else {
            return Ok(AgentCancelReceipt {
                request_id,
                session_id: self.identity.session_id.clone(),
                agent_instance_id: self.identity.agent_instance_id.clone(),
                accepted: false,
            });
        };
        self.commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::QueuedInputCancelled {
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    queued_input_id: request_id.clone(),
                    cancelled_at: now_ms(),
                },
            )
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        if let Some(input) = self.follow_ups.remove(index)
            && let Some(QueuedCompletion::Waiter { report, .. }) = input.completion
        {
            let _ = report.send(Err(AgentApiError::Cancelled));
        }
        self.publish_snapshot();
        Ok(AgentCancelReceipt {
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
            let queued_id = follow_up.durable.queued_input_id.clone();
            match self
                .start_execution_from(
                    follow_up.durable.request.clone(),
                    Some(queued_id),
                    follow_up
                        .durable
                        .detached_recipient_agent_instance_id
                        .clone(),
                )
                .await
            {
                Ok(_) => {
                    let execution_id = internal_execution_id(
                        &self.identity,
                        &follow_up.durable.request.request_id,
                    );
                    match follow_up.completion {
                        Some(QueuedCompletion::Waiter { started, report }) => {
                            let _ = started.send(());
                            self.register_waiter(execution_id, report)
                        }
                        Some(QueuedCompletion::Detached(target)) => {
                            self.register_detached_report(execution_id, target).await
                        }
                        None => {}
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
