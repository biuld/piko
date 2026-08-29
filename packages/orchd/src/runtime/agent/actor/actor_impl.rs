use super::*;

impl AgentActor {
    pub fn new(
        identity: AgentInstanceIdentity,
        spec: piko_protocol::AgentSpec,
        lifecycle: AgentInstanceLifecycle,
        transcript: Vec<piko_protocol::Message>,
        head_message_id: Option<String>,
        inbox: Vec<AgentInboxItem>,
        latest_report: Option<AgentRunReport>,
        execution_reports: Vec<piko_orchd_api::RecoveredExecutionReport>,
        queued_inputs: Vec<piko_protocol::AgentInput>,
        recovered_detached_deliveries: Vec<piko_orchd_api::RecoveredDetachedDelivery>,
        generation: u64,
        commit: Arc<dyn AgentCommitPort>,
        execution: Arc<AgentExecutionRuntime>,
        command_tx: MailboxSender<AgentCommands, AgentCommand>,
        mailbox: MailboxReceiver<AgentCommands, AgentCommand>,
        snapshot_tx: LatestSender<AgentSnapshotContract, AgentSnapshot>,
        scope: std::sync::Weak<SessionAgentScope>,
        run_cancellation: Arc<RunCancellation>,
    ) -> Self {
        Self {
            identity,
            spec,
            lifecycle,
            transcript,
            head_message_id,
            inbox,
            follow_ups: queued_inputs
                .into_iter()
                .map(|input| {
                    let completion = input.detached_recipient_agent_instance_id.as_ref().map(
                        |agent_instance_id| {
                            QueuedCompletion::Detached(DetachedReportTarget {
                                agent_instance_id: agent_instance_id.clone(),
                            })
                        },
                    );
                    QueuedRuntimeInput {
                        request: input.to_request(),
                        input,
                        completion,
                        parent: tracing::Span::none(),
                    }
                })
                .collect(),
            input_requests: HashMap::new(),
            run_state: AgentRunState::Idle,
            latest_report,
            completed_executions: execution_reports
                .into_iter()
                .map(|recovered| (recovered.internal_execution_id, recovered.report))
                .collect(),
            execution_waiters: HashMap::new(),
            detached_reports: HashMap::new(),
            scope,
            recovered_detached_deliveries,
            generation,
            commit,
            execution,
            command_tx,
            mailbox,
            snapshot_tx,
            run_cancellation,
            current_run_cancellation_generation: None,
            pending_run_parent: None,
        }
    }

    pub async fn run(mut self) {
        for delivery in std::mem::take(&mut self.recovered_detached_deliveries) {
            self.deliver_report_or_retry(DetachedDeliveryScope::new(
                delivery.recipient_agent_instance_id,
                delivery.report,
            ))
            .await;
        }
        self.advance_next_follow_up().await;
        while let Some(command) = self.mailbox.recv().await {
            match command {
                AgentCommand::Input {
                    request,
                    canonical_input,
                    reply,
                } => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let result = self
                        .handle_input(request, canonical_input)
                        .await
                        .map(|accepted| accepted.receipt);
                    command.complete(result);
                }
                AgentCommand::Run {
                    request,
                    reply,
                    parent,
                } if self.should_queue_follow_up(&request) => {
                    self.pending_run_parent = Some(parent.clone());
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let (started_tx, started_rx) = piko_comms::reply::<AgentRunStarted, _>();
                    let (report_tx, report_rx) = piko_comms::reply::<AgentRunReportContract, _>();
                    match self
                        .enqueue_follow_up(
                            request,
                            None,
                            Some(QueuedCompletion::Waiter {
                                started: started_tx,
                                report: report_tx,
                            }),
                            parent,
                        )
                        .await
                    {
                        Ok(receipt) => command.complete(Ok(piko_orchd_api::AgentRunAcceptance {
                            receipt,
                            started: started_rx,
                            completion: report_rx,
                        })),
                        Err((error, _)) => command.complete(Err(error)),
                    }
                }
                AgentCommand::Run {
                    request,
                    reply,
                    parent,
                } => {
                    self.pending_run_parent = Some(parent);
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    match self.handle_input(request, None).await {
                        Ok(accepted) => {
                            let (started_tx, started_rx) =
                                piko_comms::reply::<AgentRunStarted, _>();
                            let (report_tx, report_rx) =
                                piko_comms::reply::<AgentRunReportContract, _>();
                            self.register_waiter(accepted.internal_execution_id, report_tx);
                            let _ = started_tx.send(());
                            command.complete(Ok(piko_orchd_api::AgentRunAcceptance {
                                receipt: accepted.receipt,
                                started: started_rx,
                                completion: report_rx,
                            }));
                        }
                        Err(error) => {
                            command.complete(Err(error));
                        }
                    }
                }
                AgentCommand::InputDetached {
                    request,
                    recipient,
                    reply,
                    parent,
                } if self.should_queue_follow_up(&request) => {
                    self.pending_run_parent = Some(parent.clone());
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let result = self
                        .enqueue_follow_up(
                            request,
                            None,
                            Some(QueuedCompletion::Detached(recipient)),
                            parent,
                        )
                        .await
                        .map_err(|(error, _)| error);
                    command.complete(result);
                }
                AgentCommand::InputDetached {
                    request,
                    recipient,
                    reply,
                    parent,
                } => {
                    self.pending_run_parent = Some(parent);
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let request_execution_id =
                        internal_execution_id(&self.identity, &request.request_id);
                    let known_request = self.input_requests.contains_key(&request.request_id)
                        || self
                            .completed_executions
                            .contains_key(&request_execution_id);
                    let result = if !known_request
                        && matches!(self.run_state, AgentRunState::Idle)
                        && matches!(
                            request.delivery,
                            AgentInputDelivery::Auto
                                | AgentInputDelivery::StartWhenIdle
                                | AgentInputDelivery::FollowUp
                        ) {
                        let stored_request = request.clone();
                        let result = self
                            .start_execution_from(
                                request,
                                Some(recipient.agent_instance_id.clone()),
                                None,
                            )
                            .await
                            .map(|receipt| AcceptedAgentInput {
                                receipt,
                                internal_execution_id: request_execution_id,
                            });
                        if let Ok(accepted) = &result {
                            let canonical =
                                piko_protocol::AgentInput::from_request(&stored_request, now_ms());
                            self.input_requests.insert(
                                stored_request.request_id.clone(),
                                (stored_request, canonical, Some(accepted.clone())),
                            );
                        }
                        result
                    } else {
                        self.handle_input(request, None).await
                    };
                    if let Ok(accepted) = &result {
                        self.register_detached_report(
                            accepted.internal_execution_id.clone(),
                            recipient,
                        )
                        .await;
                    }
                    command.complete(result.map(|accepted| accepted.receipt));
                }
                AgentCommand::ExecutionFinished {
                    execution_id,
                    terminal,
                } => {
                    self.handle_execution_finished(execution_id, terminal).await;
                }
                AgentCommand::RetryTerminal { execution_id } => {
                    if self.run_state.execution_id() == Some(&execution_id) {
                        self.try_commit_terminal().await;
                    }
                }
                AgentCommand::RetryQueuedInput => self.advance_next_follow_up().await,
                AgentCommand::RetryDetachedReport { delivery } => {
                    self.deliver_report_or_retry(delivery).await;
                }
                AgentCommand::InboxReport { item } => {
                    if !self
                        .inbox
                        .iter()
                        .any(|existing| existing.report_id == item.report_id)
                    {
                        let report_id = item.report_id.clone();
                        let source_agent_instance_id = item.source_agent_instance_id.clone();
                        self.inbox.push(item);
                        self.publish_snapshot();
                        self.publish_mailbox_event(AgentMailboxEvent::InboxReport {
                            agent_instance_id: self.identity.agent_instance_id.clone(),
                            report_id,
                            source_agent_instance_id,
                        });
                    }
                }
                AgentCommand::SetLifecycle {
                    request_id,
                    lifecycle,
                    reply,
                } => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    self.lifecycle = lifecycle;
                    self.publish_snapshot();
                    command.complete(Ok(AgentLifecycleReceipt {
                        request_id,
                        session_id: self.identity.session_id.clone(),
                        agent_instance_id: self.identity.agent_instance_id.clone(),
                        lifecycle,
                    }));
                }
                AgentCommand::CancelRun { request_id, reply } => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let result = self.cancel_run(request_id).await;
                    command.complete(result);
                }
                AgentCommand::CancelInput { request_id, reply } => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let result = self.cancel_input(request_id).await;
                    command.complete(result);
                }
                AgentCommand::Inbox { reply } => {
                    let _ = reply.send(AgentInboxSnapshot {
                        session_id: self.identity.session_id.clone(),
                        agent_instance_id: self.identity.agent_instance_id.clone(),
                        items: self.inbox.clone(),
                    });
                }
                AgentCommand::ConsumeInbox { request, reply } => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let result = self.consume_inbox(request).await;
                    command.complete(result);
                }
                AgentCommand::Shutdown { reply } => {
                    let _ = reply.send(());
                    break;
                }
            }
        }
    }

    pub(super) fn should_queue_follow_up(&self, request: &SendAgentInputRequest) -> bool {
        self.run_state.execution_id().is_some() && request.delivery == AgentInputDelivery::FollowUp
    }

    pub(super) async fn enqueue_follow_up(
        &mut self,
        request: SendAgentInputRequest,
        canonical_input: Option<piko_protocol::AgentInput>,
        completion: Option<QueuedCompletion>,
        parent: tracing::Span,
    ) -> Result<AgentInputReceipt, (AgentApiError, Option<QueuedCompletion>)> {
        if self.follow_ups.len() >= MAX_QUEUED_FOLLOW_UPS {
            return Err((AgentApiError::Overload, completion));
        }
        let detached_recipient_agent_instance_id = match &completion {
            Some(QueuedCompletion::Detached(target)) => Some(target.agent_instance_id.clone()),
            _ => None,
        };
        let mut canonical_input = canonical_input
            .unwrap_or_else(|| piko_protocol::AgentInput::from_request(&request, now_ms()));
        canonical_input.detached_recipient_agent_instance_id = detached_recipient_agent_instance_id;
        let queued_input_id = canonical_input.input_id.clone();
        let request_id = request.request_id.clone();
        if let Err(error) = self
            .commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::AgentInputAdmitted {
                    admission: piko_protocol::AgentInputAdmission {
                        input: canonical_input.clone(),
                        disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
                        root_input_id: None,
                        run_id: None,
                        bound_run_id: None,
                        admitted_at: canonical_input.submitted_at,
                    },
                },
            )
            .await
        {
            return Err((
                AgentApiError::PersistenceFailed(error.to_string()),
                completion,
            ));
        }
        self.follow_ups.push_back(QueuedRuntimeInput {
            input: canonical_input,
            request,
            completion,
            parent,
        });
        self.publish_mailbox_event(AgentMailboxEvent::InputQueued {
            agent_instance_id: self.identity.agent_instance_id.clone(),
            request_id: request_id.clone(),
        });
        Ok(AgentInputReceipt {
            input_id: queued_input_id,
            request_id,
            session_id: self.identity.session_id.clone(),
            agent_instance_id: self.identity.agent_instance_id.clone(),
            disposition: InputDisposition::Queued,
            run_id: None,
            queued_position: Some(self.follow_ups.len().saturating_sub(1) as u32),
        })
    }

    /// Publish a best-effort mailbox notification after the underlying state
    /// change is durable. The session scope is only weakly held: a torn-down
    /// session simply drops the event.
    pub(super) fn publish_mailbox_event(&self, event: AgentMailboxEvent) {
        if let Some(scope) = self.scope.upgrade() {
            let _ = scope.mailbox_events().send(event);
        }
    }

    pub(super) async fn register_detached_report(
        &mut self,
        execution_id: String,
        target: DetachedReportTarget,
    ) {
        if let Some(report) = self.completed_executions.get(&execution_id).cloned() {
            self.deliver_report_or_retry(DetachedDeliveryScope::new(
                target.agent_instance_id,
                report,
            ))
            .await;
        } else {
            self.detached_reports
                .entry(execution_id)
                .or_default()
                .push(target);
        }
    }

    pub(super) fn publish_snapshot(&self) {
        let _ = self.snapshot_tx.send(AgentSnapshot {
            identity: self.identity.clone(),
            lifecycle: self.lifecycle,
            activity: if self.run_state.execution_id().is_some() {
                AgentActivity::Running
            } else {
                AgentActivity::Idle
            },
            latest_report: self.latest_report.clone(),
            unread_report_count: self
                .inbox
                .iter()
                .filter(|item| item.consumed_at.is_none())
                .count() as u32,
            generation: self.generation,
        });
    }

    pub(super) async fn cancel_run(
        &self,
        request_id: String,
    ) -> Result<piko_protocol::AgentCancelReceipt, AgentApiError> {
        let execution_id = self
            .run_state
            .execution_id()
            .ok_or(AgentApiError::InvalidState)?
            .to_string();
        if matches!(self.run_state, AgentRunState::Finalizing(_)) {
            return Ok(piko_protocol::AgentCancelReceipt {
                request_id,
                session_id: self.identity.session_id.clone(),
                agent_instance_id: self.identity.agent_instance_id.clone(),
                accepted: true,
            });
        }
        self.execution
            .request_cancel(piko_protocol::CancelExecutionRequest {
                request_id,
                session_id: self.identity.session_id.clone(),
                execution_id,
                reason: piko_protocol::CancelReason::Superseded,
            })
            .await
            .map(|receipt| piko_protocol::AgentCancelReceipt {
                request_id: receipt.request_id,
                session_id: receipt.session_id,
                agent_instance_id: self.identity.agent_instance_id.clone(),
                accepted: receipt.accepted,
            })
    }
}
