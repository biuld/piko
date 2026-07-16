use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

mod delivery;
mod input;
mod run_protocol;

use orchd_api::{AgentApiError, AgentCommitPort};
use piko_protocol::{
    AgentActivity, AgentCancelReceipt, AgentDurableCommand, AgentInboxItem, AgentInboxSnapshot,
    AgentInputDelivery, AgentInputReceipt, AgentInstanceIdentity, AgentInstanceLifecycle,
    AgentLifecycleReceipt, AgentRunReport, AgentSnapshot, ConversationContext, ExecutionConfig,
    InputDisposition, SendAgentInputRequest, StartExecutionRequest, SteerExecutionRequest,
};
use tokio::sync::{mpsc, oneshot, watch};

use super::mailbox::{AgentCommand, DetachedReportTarget};
use super::scope::SessionAgentScope;
use crate::runtime::execution::{AgentExecutionRuntime, ExecutionTerminal};
use crate::runtime::reliability::{
    ActorCommandScope, DetachedDeliveryResult, DetachedDeliveryScope, ExecutionHandoffLease,
    RunCancellation, RunStartupScope, StartedRunFailure, TerminalCommitResult, TerminalCommitScope,
};
use crate::runtime::utils::now_ms;

/// Long-lived serialization boundary for one AgentInstance.
pub struct AgentActor {
    identity: AgentInstanceIdentity,
    spec: piko_protocol::AgentSpec,
    lifecycle: AgentInstanceLifecycle,
    transcript: Vec<piko_protocol::Message>,
    head_message_id: Option<String>,
    inbox: Vec<AgentInboxItem>,
    follow_ups: VecDeque<QueuedRuntimeInput>,
    input_requests: HashMap<String, (SendAgentInputRequest, AcceptedAgentInput)>,
    run_state: AgentRunState,
    latest_report: Option<AgentRunReport>,
    completed_executions: HashMap<String, AgentRunReport>,
    execution_waiters: HashMap<String, Vec<oneshot::Sender<Result<AgentRunReport, AgentApiError>>>>,
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
    Waiter {
        started: oneshot::Sender<()>,
        report: oneshot::Sender<Result<AgentRunReport, AgentApiError>>,
    },
    Detached(DetachedReportTarget),
}

#[derive(Clone)]
struct AcceptedAgentInput {
    receipt: AgentInputReceipt,
    internal_execution_id: String,
}

enum AgentRunState {
    Idle,
    Starting { execution_id: String },
    Running { execution_id: String },
    Finalizing(TerminalCommitScope),
}

fn internal_execution_id(identity: &AgentInstanceIdentity, request_id: &str) -> String {
    orchd_api::stable_internal_id(
        "exec",
        &[
            &identity.session_id,
            &identity.agent_instance_id,
            request_id,
        ],
    )
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
        latest_report: Option<AgentRunReport>,
        execution_reports: Vec<orchd_api::RecoveredExecutionReport>,
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
                    let result = self
                        .handle_input(request)
                        .await
                        .map(|accepted| accepted.receipt);
                    command.complete(result);
                }
                AgentCommand::Run { request, reply } if self.should_queue_follow_up(&request) => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    let (started_tx, started_rx) = oneshot::channel();
                    let (report_tx, report_rx) = oneshot::channel();
                    match self
                        .enqueue_follow_up(
                            request,
                            Some(QueuedCompletion::Waiter {
                                started: started_tx,
                                report: report_tx,
                            }),
                        )
                        .await
                    {
                        Ok(receipt) => command.complete(Ok(orchd_api::AgentRunAcceptance {
                            receipt,
                            started: started_rx,
                            completion: report_rx,
                        })),
                        Err((error, _)) => command.complete(Err(error)),
                    }
                }
                AgentCommand::Run { request, reply } => {
                    let command =
                        ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                    match self.handle_input(request).await {
                        Ok(accepted) => {
                            let (started_tx, started_rx) = oneshot::channel();
                            let (report_tx, report_rx) = oneshot::channel();
                            self.register_waiter(accepted.internal_execution_id, report_tx);
                            let _ = started_tx.send(());
                            command.complete(Ok(orchd_api::AgentRunAcceptance {
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
                                None,
                                Some(recipient.agent_instance_id.clone()),
                            )
                            .await
                            .map(|receipt| AcceptedAgentInput {
                                receipt,
                                internal_execution_id: request_execution_id,
                            });
                        if let Ok(accepted) = &result {
                            self.input_requests.insert(
                                stored_request.request_id.clone(),
                                (stored_request, accepted.clone()),
                            );
                        }
                        result
                    } else {
                        self.handle_input(request).await
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
            disposition: InputDisposition::Queued,
        })
    }

    async fn register_detached_report(
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

    fn publish_snapshot(&self) {
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

    async fn cancel_run(
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
