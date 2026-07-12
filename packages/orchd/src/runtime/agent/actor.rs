use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use orchd_api::{AgentApiError, AgentCommitPort, AgentExecutor};
use piko_protocol::{
    AgentActivity, AgentDurableCommand, AgentExecutionReport, AgentInboxItem, AgentInboxSnapshot,
    AgentInputDelivery, AgentInputReceipt, AgentInstanceIdentity, AgentInstanceLifecycle,
    AgentLifecycleReceipt, AgentSnapshot, ConversationContext, ExecutionConfig, InputDisposition,
    SendAgentInputRequest, StartExecutionRequest, SteerExecutionRequest,
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::mailbox::AgentCommand;
use crate::runtime::execution::{AgentExecutionRuntime, ExecutionTerminal};

/// Long-lived serialization boundary for one AgentInstance.
pub struct AgentActor {
    identity: AgentInstanceIdentity,
    spec: piko_protocol::AgentSpec,
    lifecycle: AgentInstanceLifecycle,
    transcript: Vec<piko_protocol::Message>,
    inbox: Vec<AgentInboxItem>,
    follow_ups: VecDeque<SendAgentInputRequest>,
    input_requests: HashMap<String, (SendAgentInputRequest, AgentInputReceipt)>,
    active_execution_id: Option<String>,
    latest_report: Option<AgentExecutionReport>,
    completed_executions: HashMap<String, AgentExecutionReport>,
    generation: u64,
    commit: Arc<dyn AgentCommitPort>,
    execution: Arc<AgentExecutionRuntime>,
    command_tx: mpsc::Sender<AgentCommand>,
    mailbox: mpsc::Receiver<AgentCommand>,
    snapshot_tx: watch::Sender<AgentSnapshot>,
}

impl AgentActor {
    pub fn new(
        identity: AgentInstanceIdentity,
        spec: piko_protocol::AgentSpec,
        lifecycle: AgentInstanceLifecycle,
        transcript: Vec<piko_protocol::Message>,
        inbox: Vec<AgentInboxItem>,
        latest_report: Option<AgentExecutionReport>,
        execution_reports: Vec<AgentExecutionReport>,
        generation: u64,
        commit: Arc<dyn AgentCommitPort>,
        execution: Arc<AgentExecutionRuntime>,
        command_tx: mpsc::Sender<AgentCommand>,
        mailbox: mpsc::Receiver<AgentCommand>,
        snapshot_tx: watch::Sender<AgentSnapshot>,
    ) -> Self {
        Self {
            identity,
            spec,
            lifecycle,
            transcript,
            inbox,
            follow_ups: VecDeque::new(),
            input_requests: HashMap::new(),
            active_execution_id: None,
            latest_report,
            completed_executions: execution_reports
                .into_iter()
                .map(|report| (report.execution_id.clone(), report))
                .collect(),
            generation,
            commit,
            execution,
            command_tx,
            mailbox,
            snapshot_tx,
        }
    }

    pub async fn run(mut self) {
        while let Some(command) = self.mailbox.recv().await {
            match command {
                AgentCommand::Input { request, reply } => {
                    let result = self.handle_input(request).await;
                    let _ = reply.send(result);
                }
                AgentCommand::ExecutionFinished {
                    execution_id,
                    terminal,
                } => {
                    self.handle_execution_finished(execution_id, terminal).await;
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

    fn publish_snapshot(&self) {
        let _ = self.snapshot_tx.send(AgentSnapshot {
            identity: self.identity.clone(),
            lifecycle: self.lifecycle,
            activity: self
                .active_execution_id
                .as_ref()
                .map(|execution_id| AgentActivity::Running {
                    execution_id: execution_id.clone(),
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

        match (&self.active_execution_id, request.delivery) {
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
                let receipt = AgentInputReceipt {
                    request_id: request.request_id.clone(),
                    session_id: self.identity.session_id.clone(),
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    execution_id: Some(execution_id.clone()),
                    disposition: InputDisposition::Queued,
                };
                self.follow_ups.push_back(request);
                Ok(receipt)
            }
            (Some(execution_id), AgentInputDelivery::Auto | AgentInputDelivery::SteerActive) => {
                let execution_id = execution_id.clone();
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
        let execution_id = request
            .requested_execution_id
            .clone()
            .unwrap_or_else(|| format!("exec_{}", Uuid::new_v4()));
        self.commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::ExecutionStarted {
                    agent_instance_id: self.identity.agent_instance_id.clone(),
                    execution_id: execution_id.clone(),
                    started_at: chrono::Utc::now().timestamp_millis(),
                },
            )
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        let receipt = self
            .execution
            .start_execution(StartExecutionRequest {
                request_id: request.request_id.clone(),
                session_id: self.identity.session_id.clone(),
                // Child Executions are not Interaction Turns. This compatibility
                // field remains until ExecutionIdentity.source_turn_id lands.
                turn_id: format!("agent_run_{execution_id}"),
                execution_id: execution_id.clone(),
                agent_instance_id: self.identity.agent_instance_id.clone(),
                agent_spec: self.spec.clone(),
                input_message_id: request.message_id,
                input: request.content,
                context: ConversationContext {
                    messages: self.transcript.clone(),
                    head_message_id: None,
                    system_prompt: None,
                },
                config: ExecutionConfig {
                    agent_id: self.identity.agent_spec_id.clone(),
                    ..Default::default()
                },
            })
            .await?;
        self.active_execution_id = Some(execution_id.clone());
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
                let _ = command_tx
                    .send(AgentCommand::ExecutionFinished {
                        execution_id: watched_execution_id,
                        terminal,
                    })
                    .await;
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
    ) {
        if self.active_execution_id.as_deref() != Some(&execution_id) {
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
        if self
            .commit
            .commit_agent_command(
                &self.identity.session_id,
                AgentDurableCommand::RecordExecutionReport {
                    report: report.clone(),
                },
            )
            .await
            .is_err()
        {
            self.lifecycle = AgentInstanceLifecycle::Unavailable;
        }
        self.transcript = terminal.transcript;
        self.latest_report = Some(report);
        if let Some(report) = &self.latest_report {
            self.completed_executions
                .insert(report.execution_id.clone(), report.clone());
        }
        self.active_execution_id = None;
        self.publish_snapshot();

        if self.lifecycle == AgentInstanceLifecycle::Open
            && let Some(follow_up) = self.follow_ups.pop_front()
        {
            let _ = self.start_execution(follow_up).await;
        }
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
