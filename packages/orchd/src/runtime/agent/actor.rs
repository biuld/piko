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
    Starting {
        execution_id: String,
    },
    Running {
        execution_id: String,
    },
    Finalizing {
        execution_id: String,
        report: AgentExecutionReport,
        transcript: Vec<piko_protocol::Message>,
        head_message_id: Option<String>,
        attempts: u32,
        finished_at: i64,
        terminal_ack: Option<oneshot::Sender<()>>,
    },
}

impl AgentRunState {
    fn execution_id(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Starting { execution_id }
            | Self::Running { execution_id }
            | Self::Finalizing { execution_id, .. } => Some(execution_id),
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
        }
    }

    pub async fn run(mut self) {
        for delivery in std::mem::take(&mut self.recovered_detached_deliveries) {
            self.deliver_report_or_retry(
                DetachedReportTarget {
                    agent_instance_id: delivery.recipient_agent_instance_id,
                },
                delivery.report,
            )
            .await;
        }
        self.advance_next_follow_up().await;
        while let Some(command) = self.mailbox.recv().await {
            match command {
                AgentCommand::Input { request, reply } => {
                    let result = self.handle_input(request).await;
                    let _ = reply.send(result);
                }
                AgentCommand::Run { request, reply } if self.should_queue_follow_up(&request) => {
                    if let Err((error, completion)) = self
                        .enqueue_follow_up(request, Some(QueuedCompletion::Waiter(reply)))
                        .await
                        && let Some(QueuedCompletion::Waiter(reply)) = completion
                    {
                        let _ = reply.send(Err(error));
                    }
                }
                AgentCommand::Run { request, reply } => match self.handle_input(request).await {
                    Ok(receipt) => match receipt.execution_id {
                        Some(execution_id) => self.register_waiter(execution_id, reply),
                        None => {
                            let _ = reply.send(Err(AgentApiError::InvalidState));
                        }
                    },
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                },
                AgentCommand::InputDetached {
                    request,
                    recipient,
                    reply,
                } if self.should_queue_follow_up(&request) => {
                    let result = self
                        .enqueue_follow_up(request, Some(QueuedCompletion::Detached(recipient)))
                        .await
                        .map_err(|(error, _)| error);
                    let _ = reply.send(result);
                }
                AgentCommand::InputDetached {
                    request,
                    recipient,
                    reply,
                } => {
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
                    let _ = reply.send(result);
                }
                AgentCommand::ExecutionFinished {
                    execution_id,
                    terminal,
                    terminal_ack,
                } => {
                    self.handle_execution_finished(execution_id, terminal, Some(terminal_ack))
                        .await;
                }
                AgentCommand::RetryTerminal { execution_id } => {
                    if self.run_state.execution_id() == Some(&execution_id) {
                        self.try_commit_terminal().await;
                    }
                }
                AgentCommand::RetryQueuedInput => self.advance_next_follow_up().await,
                AgentCommand::RetryDetachedReport { target, report } => {
                    self.deliver_report_or_retry(target, report).await;
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
                    self.lifecycle = lifecycle;
                    self.publish_snapshot();
                    let _ = reply.send(Ok(AgentLifecycleReceipt {
                        request_id,
                        session_id: self.identity.session_id.clone(),
                        agent_instance_id: self.identity.agent_instance_id.clone(),
                        lifecycle,
                    }));
                }
                AgentCommand::CancelRun { request_id, reply } => {
                    let result = self.cancel_run(request_id).await;
                    let _ = reply.send(result);
                }
                AgentCommand::Inbox { reply } => {
                    let _ = reply.send(AgentInboxSnapshot {
                        session_id: self.identity.session_id.clone(),
                        agent_instance_id: self.identity.agent_instance_id.clone(),
                        items: self.inbox.clone(),
                    });
                }
                AgentCommand::ConsumeInbox { request, reply } => {
                    let result = self.consume_inbox(request).await;
                    let _ = reply.send(result);
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
        if matches!(self.run_state, AgentRunState::Finalizing { .. }) {
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
                return Err(error);
            }
        };
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
        if let Err(error) = self
            .commit
            .commit_agent_command(&self.identity.session_id, durable_start)
            .await
        {
            prepared.rollback().await;
            self.run_state = AgentRunState::Idle;
            return Err(AgentApiError::PersistenceFailed(error.to_string()));
        }
        if let Err(error) = prepared.commit_input().await {
            let receipt = prepared.receipt();
            prepared.rollback().await;
            self.run_state = AgentRunState::Running {
                execution_id: execution_id.clone(),
            };
            Box::pin(self.handle_execution_finished(
                execution_id.clone(),
                ExecutionTerminal {
                    outcome: piko_protocol::ExecutionOutcome::failed(format!(
                        "input commit failed: {error}"
                    )),
                    transcript: self.transcript.clone(),
                    head_message_id: self.head_message_id.clone(),
                },
                None,
            ))
            .await;
            return Ok(AgentInputReceipt {
                request_id: receipt.request_id,
                session_id: receipt.session_id,
                agent_instance_id: self.identity.agent_instance_id.clone(),
                execution_id: Some(execution_id),
                disposition: InputDisposition::Accepted,
            });
        }
        let receipt = match prepared.activate().await {
            Ok(receipt) => receipt,
            Err(error) => {
                self.run_state = AgentRunState::Running {
                    execution_id: execution_id.clone(),
                };
                Box::pin(self.handle_execution_finished(
                    execution_id.clone(),
                    ExecutionTerminal {
                        outcome: piko_protocol::ExecutionOutcome::failed(format!(
                            "execution activation failed: {error}"
                        )),
                        transcript: self.transcript.clone(),
                        head_message_id: self.head_message_id.clone(),
                    },
                    None,
                ))
                .await;
                return Ok(AgentInputReceipt {
                    request_id: request.request_id,
                    session_id: self.identity.session_id.clone(),
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    execution_id: Some(execution_id),
                    disposition: InputDisposition::Accepted,
                });
            }
        };
        self.run_state = AgentRunState::Running {
            execution_id: execution_id.clone(),
        };
        self.publish_snapshot();

        let execution = Arc::clone(&self.execution);
        let command_tx = self.command_tx.clone();
        let session_id = self.identity.session_id.clone();
        let watched_execution_id = execution_id.clone();
        tokio::spawn(async move {
            if let Ok(terminal) = execution
                .wait_terminal_state(&session_id, &watched_execution_id)
                .await
            {
                let (terminal_ack, acknowledged) = oneshot::channel();
                let _ = command_tx
                    .send(AgentCommand::ExecutionFinished {
                        execution_id: watched_execution_id,
                        terminal,
                        terminal_ack,
                    })
                    .await;
                let _ = acknowledged.await;
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

    async fn handle_execution_finished(
        &mut self,
        execution_id: String,
        terminal: ExecutionTerminal,
        terminal_ack: Option<oneshot::Sender<()>>,
    ) {
        if self.run_state.execution_id() != Some(&execution_id) || !self.run_state.is_running() {
            return;
        }
        let report = AgentExecutionReport {
            agent_instance_id: self.identity.agent_instance_id.clone(),
            execution_id: execution_id.clone(),
            summary: transcript_summary(&terminal.transcript),
            usage: match &terminal.outcome {
                piko_protocol::ExecutionOutcome::Succeeded { usage } => usage.clone(),
                _ => Default::default(),
            },
            outcome: terminal.outcome,
            artifacts: Vec::new(),
        };
        self.run_state = AgentRunState::Finalizing {
            execution_id,
            report,
            transcript: terminal.transcript,
            head_message_id: terminal.head_message_id,
            attempts: 0,
            finished_at: chrono::Utc::now().timestamp_millis(),
            terminal_ack,
        };
        self.try_commit_terminal().await;
    }

    async fn try_commit_terminal(&mut self) {
        let AgentRunState::Finalizing {
            execution_id,
            report,
            transcript,
            head_message_id,
            attempts,
            finished_at,
            ..
        } = &self.run_state
        else {
            return;
        };
        let execution_id = execution_id.clone();
        let report = report.clone();
        let transcript = transcript.clone();
        let head_message_id = head_message_id.clone();
        let attempts = *attempts;
        let finished_at = *finished_at;
        if let Err(error) = self
            .commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::RunTerminal {
                    run_id: execution_id.clone(),
                    report: report.clone(),
                    finished_at,
                },
            )
            .await
        {
            if matches!(
                error,
                piko_protocol::CommitError::IdentityMismatch
                    | piko_protocol::CommitError::IdempotencyConflict
            ) {
                self.lifecycle = AgentInstanceLifecycle::Unavailable;
                if let Some(waiters) = self.execution_waiters.remove(&execution_id) {
                    for waiter in waiters {
                        let _ =
                            waiter.send(Err(AgentApiError::PersistenceFailed(error.to_string())));
                    }
                }
                if let AgentRunState::Finalizing { terminal_ack, .. } = &mut self.run_state
                    && let Some(terminal_ack) = terminal_ack.take()
                {
                    let _ = terminal_ack.send(());
                }
                self.publish_snapshot();
                return;
            }
            if let AgentRunState::Finalizing { attempts, .. } = &mut self.run_state {
                *attempts = attempts.saturating_add(1);
            }
            let delay_ms = 50_u64.saturating_mul(1_u64 << attempts.min(6));
            let command_tx = self.command_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                let _ = command_tx
                    .send(AgentCommand::RetryTerminal { execution_id })
                    .await;
            });
            return;
        }
        self.transcript = transcript;
        self.head_message_id = head_message_id;
        self.latest_report = Some(report.clone());
        if let Some(report) = &self.latest_report {
            self.completed_executions
                .insert(report.execution_id.clone(), report.clone());
        }
        if let AgentRunState::Finalizing { terminal_ack, .. } = &mut self.run_state
            && let Some(terminal_ack) = terminal_ack.take()
        {
            let _ = terminal_ack.send(());
        }
        self.run_state = AgentRunState::Idle;
        self.publish_snapshot();

        if let Some(waiters) = self.execution_waiters.remove(&execution_id) {
            for waiter in waiters {
                let _ = waiter.send(Ok(report.clone()));
            }
        }
        if let Some(targets) = self.detached_reports.remove(&execution_id) {
            for target in targets {
                self.deliver_report_or_retry(target, report.clone()).await;
            }
        }

        self.advance_next_follow_up().await;
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

    async fn deliver_report_or_retry(
        &self,
        target: DetachedReportTarget,
        report: AgentExecutionReport,
    ) {
        if self.deliver_report(&target, &report).await {
            return;
        }
        let command_tx = self.command_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let _ = command_tx
                .send(AgentCommand::RetryDetachedReport { target, report })
                .await;
        });
    }

    async fn deliver_report(
        &self,
        target: &DetachedReportTarget,
        report: &AgentExecutionReport,
    ) -> bool {
        if self
            .commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::CommitReport {
                    recipient_agent_instance_id: target.agent_instance_id.clone(),
                    report: report.clone(),
                },
            )
            .await
            .is_err()
        {
            return false;
        }
        let Some(scope) = self.scope.upgrade() else {
            return false;
        };
        let Some(recipient) = scope.agent(&target.agent_instance_id).await else {
            return false;
        };
        recipient
            .command_tx
            .send(AgentCommand::InboxReport {
                item: AgentInboxItem {
                    report_id: format!(
                        "report_{}_{}",
                        report.agent_instance_id, report.execution_id
                    ),
                    recipient_agent_instance_id: target.agent_instance_id.clone(),
                    source_agent_instance_id: report.agent_instance_id.clone(),
                    report: report.clone(),
                    committed_at: chrono::Utc::now().timestamp_millis(),
                    consumed_at: None,
                },
            })
            .await
            .is_ok()
    }
}

fn transcript_summary(transcript: &[piko_protocol::Message]) -> String {
    transcript
        .iter()
        .rev()
        .find_map(|message| match message {
            piko_protocol::Message::Assistant { content, .. } => Some(
                content
                    .iter()
                    .filter_map(|block| match block {
                        piko_protocol::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            ),
            _ => None,
        })
        .unwrap_or_default()
}
