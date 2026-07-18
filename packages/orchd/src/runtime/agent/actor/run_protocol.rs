use super::*;

impl AgentActor {
    pub(super) async fn start_execution(
        &mut self,
        request: SendAgentInputRequest,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        self.start_execution_from(request, None, None).await
    }

    pub(super) async fn start_execution_from(
        &mut self,
        request: SendAgentInputRequest,
        queued_input_id: Option<String>,
        detached_recipient_agent_instance_id: Option<String>,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        let execution_id = internal_execution_id(&self.identity, &request.request_id);
        self.run_state = AgentRunState::Starting {
            execution_id: execution_id.clone(),
        };
        let (cancellation_generation, startup_cancel) = self.run_cancellation.begin();
        self.current_run_cancellation_generation = Some(cancellation_generation);
        self.publish_snapshot();
        let run_context = match self
            .execution
            .prepare_run_context(&request, &self.spec)
            .await
        {
            Ok(context) => context,
            Err(error) => {
                self.run_state = AgentRunState::Idle;
                self.finish_run_cancellation();
                return Err(error);
            }
        };
        let prompt_assembly_version = run_context.prompt.assembly_version;
        let prompt_digest = run_context.prompt.source_digest.clone();
        let prepared = match self
            .execution
            .prepare_execution(
                StartExecutionRequest {
                    request_id: request.request_id.clone(),
                    session_id: self.identity.session_id.clone(),
                    source_turn_id: request.source_turn_id.clone(),
                    execution_id: execution_id.clone(),
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    agent_spec: self.spec.clone(),
                    run_prompt: run_context.prompt,
                    tool_catalog: run_context.tool_catalog,
                    input_message_id: request.message_id,
                    input: request.content,
                    context: ConversationContext {
                        messages: self.transcript.clone(),
                        head_message_id: self.head_message_id.clone(),
                    },
                    config: ExecutionConfig {
                        agent_id: self.identity.agent_spec_id.clone(),
                        ..Default::default()
                    },
                },
                run_context.routes,
            )
            .await
        {
            Ok(prepared) => prepared,
            Err(error) => {
                self.run_state = AgentRunState::Idle;
                self.finish_run_cancellation();
                return Err(error);
            }
        };
        if startup_cancel.is_cancelled() {
            prepared.rollback().await;
            self.run_state = AgentRunState::Idle;
            self.finish_run_cancellation();
            return Err(AgentApiError::Cancelled);
        }
        let durable_start = match queued_input_id {
            Some(queued_input_id) => AgentDurableCommand::QueuedInputStarted {
                agent_instance_id: self.identity.agent_instance_id.clone(),
                queued_input_id,
                run_id: execution_id.clone(),
                internal_execution_id: execution_id.clone(),
                request_id: request.request_id.clone(),
                source_turn_id: request.source_turn_id.clone(),
                detached_recipient_agent_instance_id: detached_recipient_agent_instance_id.clone(),
                prompt_assembly_version,
                prompt_digest: prompt_digest.clone(),
                started_at: chrono::Utc::now().timestamp_millis(),
            },
            None => AgentDurableCommand::RunStarted {
                agent_instance_id: self.identity.agent_instance_id.clone(),
                run_id: execution_id.clone(),
                internal_execution_id: execution_id.clone(),
                request_id: request.request_id.clone(),
                source_turn_id: request.source_turn_id.clone(),
                detached_recipient_agent_instance_id: detached_recipient_agent_instance_id.clone(),
                prompt_assembly_version,
                prompt_digest,
                started_at: chrono::Utc::now().timestamp_millis(),
            },
        };
        let startup = match RunStartupScope::new(prepared)
            .commit_start(&self.commit, &self.identity.session_id, durable_start)
            .await
        {
            Ok(startup) => startup,
            Err(error) => {
                self.run_state = AgentRunState::Idle;
                self.finish_run_cancellation();
                return Err(error);
            }
        };
        let startup = match startup.commit_input().await {
            Ok(startup) => startup,
            Err(failure) => {
                return self
                    .finish_failed_started_run(execution_id, "input commit failed", failure)
                    .await;
            }
        };
        if startup_cancel.is_cancelled() {
            let receipt = startup.receipt();
            let (committed_input, input_message_id) = startup.committed_input();
            startup.rollback().await;
            return self
                .finish_cancelled_started_run(
                    execution_id,
                    committed_input,
                    input_message_id,
                    receipt,
                )
                .await;
        }
        let receipt = startup.activate().await;
        self.run_state = AgentRunState::Running {
            execution_id: execution_id.clone(),
        };
        self.publish_snapshot();
        if startup_cancel.is_cancelled() {
            let _ = self
                .execution
                .request_cancel(piko_protocol::CancelExecutionRequest {
                    request_id: format!("cancel-startup-{execution_id}"),
                    session_id: self.identity.session_id.clone(),
                    execution_id: execution_id.clone(),
                    reason: piko_protocol::CancelReason::Superseded,
                })
                .await;
        }

        let execution = Arc::clone(&self.execution);
        let command_tx = self.command_tx.clone();
        let session_id = self.identity.session_id.clone();
        let watched_execution_id = execution_id.clone();
        tokio::spawn(async move {
            if let Ok(terminal) = execution
                .wait_terminal_state(&session_id, &watched_execution_id)
                .await
            {
                let (terminal, acknowledged) = ExecutionHandoffLease::new(terminal);
                let _ = command_tx
                    .send(AgentCommand::ExecutionFinished {
                        execution_id: watched_execution_id,
                        terminal,
                    })
                    .await;
                let _ = acknowledged.wait().await;
            }
        });

        Ok(AgentInputReceipt {
            request_id: receipt.request_id,
            session_id: receipt.session_id,
            agent_instance_id: self.identity.agent_instance_id.clone(),
            disposition: InputDisposition::Accepted,
        })
    }

    async fn finish_failed_started_run(
        &mut self,
        execution_id: String,
        context: &str,
        failure: StartedRunFailure,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        self.run_state = AgentRunState::Running {
            execution_id: execution_id.clone(),
        };
        let (terminal, _unobserved) = ExecutionHandoffLease::new(ExecutionTerminal {
            outcome: piko_protocol::ExecutionOutcome::failed(format!(
                "{context}: {}",
                failure.error
            )),
            transcript: self.transcript.clone(),
            head_message_id: self.head_message_id.clone(),
        });
        Box::pin(self.handle_execution_finished(execution_id, terminal)).await;
        Ok(failure.receipt)
    }

    async fn finish_cancelled_started_run(
        &mut self,
        execution_id: String,
        committed_input: piko_protocol::Message,
        input_message_id: String,
        receipt: AgentInputReceipt,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        self.run_state = AgentRunState::Running {
            execution_id: execution_id.clone(),
        };
        let mut transcript = self.transcript.clone();
        transcript.push(committed_input);
        let (terminal, _unobserved) = ExecutionHandoffLease::new(ExecutionTerminal {
            outcome: piko_protocol::ExecutionOutcome::Cancelled {
                reason: Some("cancelled during startup".into()),
            },
            transcript,
            head_message_id: Some(input_message_id),
        });
        Box::pin(self.handle_execution_finished(execution_id, terminal)).await;
        Ok(receipt)
    }

    fn finish_run_cancellation(&mut self) {
        if let Some(generation) = self.current_run_cancellation_generation.take() {
            self.run_cancellation.finish(generation);
        }
    }

    pub(super) async fn handle_execution_finished(
        &mut self,
        execution_id: String,
        terminal: ExecutionHandoffLease<ExecutionTerminal>,
    ) {
        if self.run_state.execution_id() != Some(&execution_id) || !self.run_state.is_running() {
            return;
        }
        self.run_state = AgentRunState::Finalizing(TerminalCommitScope::new(
            execution_id,
            self.identity.agent_instance_id.clone(),
            terminal,
        ));
        self.try_commit_terminal().await;
    }

    pub(super) async fn try_commit_terminal(&mut self) {
        let AgentRunState::Finalizing(terminal) = &mut self.run_state else {
            return;
        };
        match terminal
            .commit(&self.commit, &self.identity.session_id)
            .await
        {
            TerminalCommitResult::PermanentFailure(mut failure) => {
                self.lifecycle = AgentInstanceLifecycle::Unavailable;
                self.finish_run_cancellation();
                if let Some(waiters) = self.execution_waiters.remove(&failure.execution_id) {
                    for waiter in waiters {
                        let _ = waiter.send(Err(AgentApiError::PersistenceFailed(
                            failure.error.to_string(),
                        )));
                    }
                }
                failure.acknowledge_handoff();
                self.publish_snapshot();
            }
            TerminalCommitResult::Retry {
                execution_id,
                delay_ms,
            } => {
                let command_tx = self.command_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    let _ = command_tx
                        .send(AgentCommand::RetryTerminal { execution_id })
                        .await;
                });
            }
            TerminalCommitResult::Committed(mut committed) => {
                self.transcript = committed.transcript.clone();
                self.head_message_id = committed.head_message_id.clone();
                self.latest_report = Some(committed.report.clone());
                self.completed_executions
                    .insert(committed.execution_id.clone(), committed.report.clone());
                committed.acknowledge_handoff();
                self.run_state = AgentRunState::Idle;
                self.finish_run_cancellation();
                self.publish_snapshot();

                if let Some(waiters) = self.execution_waiters.remove(&committed.execution_id) {
                    for waiter in waiters {
                        let _ = waiter.send(Ok(committed.report.clone()));
                    }
                }
                if let Some(targets) = self.detached_reports.remove(&committed.execution_id) {
                    for target in targets {
                        self.deliver_report_or_retry(DetachedDeliveryScope::new(
                            target.agent_instance_id,
                            committed.report.clone(),
                        ))
                        .await;
                    }
                }

                self.advance_next_follow_up().await;
            }
        }
    }
}
