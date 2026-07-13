use super::*;

impl AgentActor {
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
                Ok(receipt) => {
                    if let Some(execution_id) = receipt.execution_id {
                        match follow_up.completion {
                            Some(QueuedCompletion::Waiter(waiter)) => {
                                self.register_waiter(execution_id, waiter)
                            }
                            Some(QueuedCompletion::Detached(target)) => self
                                .detached_reports
                                .entry(execution_id)
                                .or_default()
                                .push(target),
                            None => {}
                        }
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
