use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

mod delivery;
mod input;
mod run_protocol;

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
}
