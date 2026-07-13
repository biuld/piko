use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use orchd_api::{AgentApiError, AgentCommitPort};
use piko_protocol::{
    AgentActivity, AgentDurableCommand, AgentExecutionReport, AgentInboxItem, AgentInboxSnapshot,
    AgentInputDelivery, AgentInputReceipt, AgentInstanceIdentity, AgentInstanceLifecycle,
    AgentLifecycleReceipt, AgentSnapshot, ConversationContext, ExecutionConfig, InputDisposition,
    SendAgentInputRequest, StartExecutionRequest, SteerExecutionRequest,
};
use tokio::sync::{mpsc, oneshot, watch};
use uuid::Uuid;

use super::mailbox::{AgentCommand, DetachedReportTarget};
use super::scope::SessionAgentScope;
use crate::runtime::execution::{AgentExecutionRuntime, ExecutionTerminal};
use crate::runtime::reliability::{
    ActorCommandScope, DetachedDeliveryResult, DetachedDeliveryScope, ExecutionHandoffLease,
    RunCancellation, RunStartupScope, StartedRunFailure, TerminalCommitResult, TerminalCommitScope,
};

/// Long-lived serialization boundary for one AgentInstance.
pub struct AgentActor {
    identity: AgentInstanceIdentity,
    spec: piko_protocol::AgentSpec,
    lifecycle: AgentInstanceLifecycle,
    transcript: Vec<piko_protocol::Message>,
    head_message_id: Option<String>,
    inbox: Vec<AgentInboxItem>,
    follow_ups: VecDeque<QueuedRuntimeInput>,
    input_requests: HashMap<String, (SendAgentInputRequest, AgentInputReceipt)>,
    run_state: AgentRunState,
    latest_report: Option<AgentExecutionReport>,
    completed_executions: HashMap<String, AgentExecutionReport>,
    execution_waiters:
        HashMap<String, Vec<oneshot::Sender<Result<AgentExecutionReport, AgentApiError>>>>,
    detached_reports: HashMap<String, Vec<DetachedReportTarget>>,
    scope: std::sync::Weak<SessionAgentScope>,
    recovered_detached_deliveries: Vec<orchd_api::RecoveredDetachedDelivery>,
    generation: u64,
    commit: Arc<dyn AgentCommitPort>,
    execution: Arc<AgentExecutionRuntime>,
    command_tx: mpsc::Sender<AgentCommand>,
    mailbox: mpsc::Receiver<AgentCommand>,
    snapshot_tx: watch::Sender<AgentSnapshot>,
    run_cancellation: Arc<RunCancellation>,
    current_run_cancellation_generation: Option<u64>,
}

struct QueuedRuntimeInput {
    durable: piko_protocol::DurableAgentInput,
    completion: Option<QueuedCompletion>,
}

enum QueuedCompletion {
    Waiter(oneshot::Sender<Result<AgentExecutionReport, AgentApiError>>),
    Detached(DetachedReportTarget),
}

enum AgentRunState {
    Idle,
    Starting { execution_id: String },
    Running { execution_id: String },
    Finalizing(TerminalCommitScope),
}

impl AgentRunState {
    fn execution_id(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Starting { execution_id } | Self::Running { execution_id } => Some(execution_id),
            Self::Finalizing(terminal) => Some(terminal.execution_id()),
        }
    }

    fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }
}

impl AgentActor {
    pub fn new(
        identity: AgentInstanceIdentity,
        spec: piko_protocol::AgentSpec,
        lifecycle: AgentInstanceLifecycle,
        transcript: Vec<piko_protocol::Message>,
        head_message_id: Option<String>,
        inbox: Vec<AgentInboxItem>,
        latest_report: Option<AgentExecutionReport>,
        execution_reports: Vec<AgentExecutionReport>,
        queued_inputs: Vec<piko_protocol::DurableAgentInput>,
        recovered_detached_deliveries: Vec<orchd_api::RecoveredDetachedDelivery>,
        generation: u64,
        commit: Arc<dyn AgentCommitPort>,
        execution: Arc<AgentExecutionRuntime>,
        command_tx: mpsc::Sender<AgentCommand>,
        mailbox: mpsc::Receiver<AgentCommand>,
        snapshot_tx: watch::Sender<AgentSnapshot>,
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
                .map(|durable| {
                    let completion = durable.detached_recipient_agent_instance_id.as_ref().map(
                        |agent_instance_id| {
                            QueuedCompletion::Detached(DetachedReportTarget {
                                agent_instance_id: agent_instance_id.clone(),
                            })
                        },
                    );
                    QueuedRuntimeInput {
                        durable,
                        completion,
                    }
                })
                .collect(),
            input_requests: HashMap::new(),
            run_state: AgentRunState::Idle,
            latest_report,
            completed_executions: execution_reports
                .into_iter()
                .map(|report| (report.execution_id.clone(), report))
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
                AgentCommand::Input { request, reply } => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let result = self.handle_input(request).await;
                    command.complete(result);
                }
                AgentCommand::Run { request, reply } if self.should_queue_follow_up(&request) => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    if let Err((error, completion)) = self
                        .enqueue_follow_up(
                            request,
                            Some(QueuedCompletion::Waiter(command.transfer())),
                        )
                        .await
                        && let Some(QueuedCompletion::Waiter(reply)) = completion
                    {
                        let _ = reply.send(Err(error));
                    }
                }
                AgentCommand::Run { request, reply } => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    match self.handle_input(request).await {
                        Ok(receipt) => match receipt.execution_id {
                            Some(execution_id) => {
                                self.register_waiter(execution_id, command.transfer())
                            }
                            None => {
                                command.complete(Err(AgentApiError::InvalidState));
                            }
                        },
                        Err(error) => {
                            command.complete(Err(error));
                        }
                    }
                }
                AgentCommand::InputDetached {
                    request,
                    recipient,
                    reply,
                } if self.should_queue_follow_up(&request) => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let result = self
                        .enqueue_follow_up(request, Some(QueuedCompletion::Detached(recipient)))
                        .await
                        .map_err(|(error, _)| error);
                    command.complete(result);
                }
                AgentCommand::InputDetached {
                    request,
                    recipient,
                    reply,
                } => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let result = if matches!(self.run_state, AgentRunState::Idle)
                        && matches!(
                            request.delivery,
                            AgentInputDelivery::Auto
                                | AgentInputDelivery::StartWhenIdle
                                | AgentInputDelivery::FollowUp
                        ) {
                        self.start_execution_from(
                            request,
                            None,
                            Some(recipient.agent_instance_id.clone()),
                        )
                        .await
                    } else {
                        self.handle_input(request).await
                    };
                    if let Ok(receipt) = &result
                        && let Some(execution_id) = &receipt.execution_id
                    {
                        self.detached_reports
                            .entry(execution_id.clone())
                            .or_default()
                            .push(recipient);
                    }
                    command.complete(result);
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
                        self.inbox.push(item);
                        self.publish_snapshot();
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

    fn should_queue_follow_up(&self, request: &SendAgentInputRequest) -> bool {
        self.run_state.execution_id().is_some() && request.delivery == AgentInputDelivery::FollowUp
    }

    async fn enqueue_follow_up(
        &mut self,
        request: SendAgentInputRequest,
        completion: Option<QueuedCompletion>,
    ) -> Result<AgentInputReceipt, (AgentApiError, Option<QueuedCompletion>)> {
        let detached_recipient_agent_instance_id = match &completion {
            Some(QueuedCompletion::Detached(target)) => Some(target.agent_instance_id.clone()),
            _ => None,
        };
        let durable = piko_protocol::DurableAgentInput {
            queued_input_id: request.request_id.clone(),
            request: request.clone(),
            detached_recipient_agent_instance_id,
        };
        if let Err(error) = self
            .commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::InputQueued {
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    queued_input: durable.clone(),
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
            durable,
            completion,
        });
        Ok(AgentInputReceipt {
            request_id: request.request_id,
            session_id: self.identity.session_id.clone(),
            agent_instance_id: self.identity.agent_instance_id.clone(),
            execution_id: None,
            disposition: InputDisposition::Queued,
        })
    }

    fn register_waiter(
        &mut self,
        execution_id: String,
        reply: oneshot::Sender<Result<AgentExecutionReport, AgentApiError>>,
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

    fn publish_snapshot(&self) {
        let _ = self.snapshot_tx.send(AgentSnapshot {
            identity: self.identity.clone(),
            lifecycle: self.lifecycle,
            activity: self
                .run_state
                .execution_id()
                .map(|execution_id| AgentActivity::Running {
                    execution_id: execution_id.to_string(),
                })
                .unwrap_or(AgentActivity::Idle),
            latest_report: self.latest_report.clone(),
            unread_report_count: self
                .inbox
                .iter()
                .filter(|item| item.consumed_at.is_none())
                .count() as u32,
            generation: self.generation,
        });
    }

    async fn cancel_run(
        &self,
        request_id: String,
    ) -> Result<piko_protocol::CancelReceipt, AgentApiError> {
        let execution_id = self
            .run_state
            .execution_id()
            .ok_or(AgentApiError::InvalidState)?
            .to_string();
        if matches!(self.run_state, AgentRunState::Finalizing(_)) {
            return Ok(piko_protocol::CancelReceipt {
                request_id,
                session_id: self.identity.session_id.clone(),
                execution_id,
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
    }

    async fn handle_input(
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

    async fn consume_inbox(
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

    async fn start_execution(
        &mut self,
        request: SendAgentInputRequest,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        self.start_execution_from(request, None, None).await
    }

    async fn start_execution_from(
        &mut self,
        request: SendAgentInputRequest,
        queued_input_id: Option<String>,
        detached_recipient_agent_instance_id: Option<String>,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        let execution_id = request
            .requested_execution_id
            .clone()
            .unwrap_or_else(|| format!("exec_{}", Uuid::new_v4()));
        self.run_state = AgentRunState::Starting {
            execution_id: execution_id.clone(),
        };
        let (cancellation_generation, startup_cancel) = self.run_cancellation.begin();
        self.current_run_cancellation_generation = Some(cancellation_generation);
        self.publish_snapshot();
        let prepared = match self
            .execution
            .prepare_execution(StartExecutionRequest {
                request_id: request.request_id.clone(),
                session_id: self.identity.session_id.clone(),
                source_turn_id: request.source_turn_id.clone(),
                execution_id: execution_id.clone(),
                agent_instance_id: self.identity.agent_instance_id.clone(),
                agent_spec: self.spec.clone(),
                input_message_id: request.message_id,
                input: request.content,
                context: ConversationContext {
                    messages: self.transcript.clone(),
                    head_message_id: self.head_message_id.clone(),
                    system_prompt: None,
                },
                config: ExecutionConfig {
                    agent_id: self.identity.agent_spec_id.clone(),
                    ..Default::default()
                },
            })
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
                started_at: chrono::Utc::now().timestamp_millis(),
            },
            None => AgentDurableCommand::RunStarted {
                agent_instance_id: self.identity.agent_instance_id.clone(),
                run_id: execution_id.clone(),
                internal_execution_id: execution_id.clone(),
                request_id: request.request_id.clone(),
                source_turn_id: request.source_turn_id.clone(),
                detached_recipient_agent_instance_id: detached_recipient_agent_instance_id.clone(),
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
            execution_id: Some(execution_id),
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

    async fn handle_execution_finished(
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

    async fn try_commit_terminal(&mut self) {
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

    async fn advance_next_follow_up(&mut self) {
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

    async fn deliver_report_or_retry(&self, mut delivery: DetachedDeliveryScope) {
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
