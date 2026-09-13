use super::*;

/// Startup failure classification for queued follow-ups.
enum QueuedInputFailure {
    /// Never succeeds on retry: the input gets a terminal disposition and
    /// the queue advances.
    Permanent,
    /// Transient infrastructure failure: requeue with bounded backoff.
    Retryable,
}

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
        let Some(mut follow_up) = self.follow_ups.pop_front() else {
            return;
        };
        if follow_up.terminal_failure.is_some() {
            self.commit_failed_follow_up(follow_up).await;
            return;
        }
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
            Err(error) => {
                // A permanently failed input must never block the queue: it is
                // given a terminal disposition and the next follow-up runs.
                // Retryable failures requeue with bounded backoff.
                match Self::classify_startup_failure(&error) {
                    QueuedInputFailure::Permanent => {
                        tracing::warn!(
                            session_id = %self.identity.session_id,
                            agent_instance_id = %self.identity.agent_instance_id,
                            input_id = %follow_up.input.input_id,
                            error = %error,
                            "queued follow-up failed permanently; cancelling input"
                        );
                        follow_up.terminal_failure = Some(error.to_string());
                        self.commit_failed_follow_up(follow_up).await;
                    }
                    QueuedInputFailure::Retryable => {
                        let delay_ms = follow_up.retry.next_delay_ms();
                        tracing::warn!(
                            session_id = %self.identity.session_id,
                            agent_instance_id = %self.identity.agent_instance_id,
                            input_id = %follow_up.input.input_id,
                            delay_ms,
                            error = %error,
                            "queued follow-up start failed; retrying with backoff"
                        );
                        self.follow_ups.push_front(follow_up);
                        let command_tx = self.command_tx.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                            let _ = command_tx.send(AgentCommand::RetryQueuedInput).await;
                        });
                    }
                }
            }
        }
    }

    /// Keep a permanently failed follow-up queued until its terminal
    /// disposition is durable. Dropping it after a transient commit failure
    /// would resurrect the pending input on the next recovery.
    async fn commit_failed_follow_up(&mut self, mut follow_up: QueuedRuntimeInput) {
        let result = self
            .commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::AgentInputDispositionChanged {
                    change: piko_protocol::AgentInputDispositionChange {
                        agent_instance_id: self.identity.agent_instance_id.clone(),
                        input_id: follow_up.input.input_id.clone(),
                        disposition: piko_protocol::AgentInputDisposition::Cancelled,
                        root_input_id: None,
                        model_step_id: None,
                        changed_at: now_ms(),
                    },
                },
            )
            .await;
        match result {
            Ok(_) => {
                self.publish_snapshot();
                let command_tx = self.command_tx.clone();
                tokio::spawn(async move {
                    let _ = command_tx.send(AgentCommand::RetryQueuedInput).await;
                });
            }
            Err(commit_error) => {
                let delay_ms = follow_up.retry.next_delay_ms();
                tracing::error!(
                    session_id = %self.identity.session_id,
                    input_id = %follow_up.input.input_id,
                    startup_error = follow_up.terminal_failure.as_deref().unwrap_or("unknown"),
                    error = %commit_error,
                    delay_ms,
                    "terminal disposition for failed follow-up could not be committed; retrying"
                );
                self.follow_ups.push_front(follow_up);
                self.publish_snapshot();
                let command_tx = self.command_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    let _ = command_tx.send(AgentCommand::RetryQueuedInput).await;
                });
            }
        }
    }

    /// Distinguish failures that will never succeed on retry from transient
    /// infrastructure failures. Only `start_execution_from` (prompt
    /// assembly, tool catalog, context budget, admission state) can fail here;
    /// durable admission has not happened yet, so cancelling the pending
    /// follow-up is the correct terminal disposition.
    fn classify_startup_failure(error: &AgentApiError) -> QueuedInputFailure {
        match error {
            AgentApiError::ContextBudgetExceeded(_)
            | AgentApiError::InputRejected
            | AgentApiError::ToolCatalogFailed(_)
            | AgentApiError::PromptAssemblyFailed(_)
            | AgentApiError::AgentClosed
            | AgentApiError::AgentTerminated
            | AgentApiError::InvalidState
            | AgentApiError::IdempotencyConflict
            | AgentApiError::ExecutionAlreadyActive
            | AgentApiError::AgentSpecNotFound => QueuedInputFailure::Permanent,
            AgentApiError::Overload
            | AgentApiError::RuntimeUnavailable
            | AgentApiError::PersistenceFailed(_) => QueuedInputFailure::Retryable,
            // A cancelled startup consumed a durable start; the input is
            // already terminally bound by the cancellation path.
            AgentApiError::Cancelled => QueuedInputFailure::Permanent,
            _ => QueuedInputFailure::Retryable,
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
