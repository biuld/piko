use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use orchd_api::{AgentApiError, CancelReceipt, ExecutionInputReceipt, InputDisposition};
use piko_protocol::execution::{
    ExecutionOutcome, ExecutionSnapshot, ExecutionStatus, StartExecutionRequest,
    SteerExecutionRequest,
};
use piko_protocol::{Message, Usage};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use super::ExecutionIdentity;
use super::mailbox::ExecutionCommand;
use super::scope::SessionExecutionScope;
use super::services::ExecutionServices;
use super::state::ExecutionState;
use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry};
use crate::domain::model::step::ModelSpec;
use crate::domain::tools::call::{ToolCall, ToolCallItem};
use crate::domain::transcript::TranscriptManager;
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext};
use crate::runtime::events::identity::DispatchIdentity;
use crate::runtime::runtime_assistant_message_id;
use crate::runtime::step::StepDispatch;
use crate::runtime::tools::{append_tool, append_tool_err};
use crate::runtime::utils::runtime_tool_entity_id;
use llmd::gateway::GatewayRequest;

pub struct ExecutionActor {
    identity: ExecutionIdentity,
    state: ExecutionState,
    mailbox: mpsc::Receiver<ExecutionCommand>,
    cancel: CancellationToken,
    ports: Arc<SessionExecutionScope>,
    services: ExecutionServices,
    snapshot_tx: watch::Sender<ExecutionSnapshot>,
    request: StartExecutionRequest,
}

impl ExecutionActor {
    pub fn new(
        identity: ExecutionIdentity,
        request: StartExecutionRequest,
        mailbox: mpsc::Receiver<ExecutionCommand>,
        cancel: CancellationToken,
        ports: Arc<SessionExecutionScope>,
        services: ExecutionServices,
        snapshot_tx: watch::Sender<ExecutionSnapshot>,
    ) -> Self {
        let mut transcript = TranscriptManager::new(Some(request.context.messages.clone()));
        transcript.push_user_content(request.input.clone(), None);
        let state = ExecutionState {
            status: ExecutionStatus::Accepted,
            transcript,
            model_step_index: 0,
            steering: VecDeque::new(),
            usage: Usage::default(),
            // Host commits the user input before start_execution; durable head for
            // this execution is always that input message, not prior-turn context head.
            head_message_id: Some(request.input_message_id.clone()),
            error: None,
        };
        Self {
            identity,
            state,
            mailbox,
            cancel,
            ports,
            services,
            snapshot_tx,
            request,
        }
    }

    pub fn identity(&self) -> &ExecutionIdentity {
        &self.identity
    }

    pub async fn run(mut self) -> Result<ExecutionOutcome, AgentApiError> {
        self.transition(ExecutionStatus::Running);
        self.publish_snapshot();

        loop {
            if self.cancel.is_cancelled() {
                return Ok(ExecutionOutcome::Cancelled {
                    reason: Some("cancelled".into()),
                });
            }

            self.drain_controls_nonblocking()?;

            let step = self.run_model_step().await?;
            self.commit_assistant(&step.assistant_message, &step.message_id)
                .await?;
            self.state.transcript.push_assistant(step.assistant_message);

            if !step.tool_calls.is_empty() {
                if !self.request.config.allow_tool_calls {
                    return Err(AgentApiError::InputRejected);
                }
                self.execute_and_commit_tools(&step.tool_calls, &step.routes, &step.message_id)
                    .await?;
                self.drain_controls_at_step_boundary().await?;
                if let Some(steering) = self.state.steering.pop_front() {
                    self.commit_steering(&steering).await?;
                }
                continue;
            }

            self.drain_controls_at_step_boundary().await?;

            if let Some(steering) = self.state.steering.pop_front() {
                self.commit_steering(&steering).await?;
                continue;
            }

            return Ok(ExecutionOutcome::Succeeded {
                usage: self.state.usage.clone(),
            });
        }
    }

    fn transition(&mut self, status: ExecutionStatus) {
        self.state.status = status;
    }

    fn publish_snapshot(&self) {
        let _ = self.snapshot_tx.send(ExecutionSnapshot {
            session_id: self.identity.session_id.clone(),
            turn_id: self.identity.turn_id.clone(),
            execution_id: self.identity.execution_id.clone(),
            agent_id: self.identity.agent_id.clone(),
            status: self.state.status.clone(),
            model_step_index: self.state.model_step_index,
            usage: self.state.usage.clone(),
            error: self.state.error.clone(),
        });
    }

    fn drain_controls_nonblocking(&mut self) -> Result<(), AgentApiError> {
        while let Ok(command) = self.mailbox.try_recv() {
            self.handle_command(command)?;
        }
        Ok(())
    }

    async fn drain_controls_at_step_boundary(&mut self) -> Result<(), AgentApiError> {
        self.drain_controls_nonblocking()?;
        Ok(())
    }

    fn handle_command(&mut self, command: ExecutionCommand) -> Result<(), AgentApiError> {
        match command {
            ExecutionCommand::Steer { request, reply } => {
                let receipt = ExecutionInputReceipt {
                    request_id: request.request_id.clone(),
                    session_id: self.identity.session_id.clone(),
                    execution_id: self.identity.execution_id.clone(),
                    message_id: request.message_id.clone(),
                    disposition: InputDisposition::Queued,
                };
                self.state.steering.push_back(request);
                let _ = reply.send(Ok(receipt));
            }
            ExecutionCommand::Cancel {
                request_id,
                reason: _,
                reply,
            } => {
                self.cancel.cancel();
                let _ = reply.send(Ok(CancelReceipt {
                    request_id,
                    session_id: self.identity.session_id.clone(),
                    execution_id: self.identity.execution_id.clone(),
                    accepted: true,
                }));
            }
            ExecutionCommand::Shutdown { reply } => {
                self.cancel.cancel();
                let _ = reply.send(());
            }
        }
        Ok(())
    }

    async fn run_model_step(&mut self) -> Result<CompletedModelStep, AgentApiError> {
        self.state.model_step_index += 1;
        let step_count = self.state.model_step_index;
        let message_id = runtime_assistant_message_id(
            &self.identity.execution_id,
            &format!("step_{step_count}"),
        );

        let agent = self
            .services
            .agent_spec(&self.request.config.agent_id)
            .await
            .ok_or(AgentApiError::InvalidState)?;

        let model = self.resolve_model(&agent).await;
        let (tools, routes) = if self.request.config.allow_tool_calls {
            (*self.services.tool_registry())
                .discover_tools(&ToolDiscoveryContext {
                    agent_id: agent.id.clone(),
                    task_id: Some(self.identity.execution_id.clone()),
                    tool_set_ids: agent.tool_set_ids.clone(),
                    active_tool_names: agent.active_tool_names.clone(),
                })
                .await
        } else {
            (Vec::new(), HashMap::new())
        };

        let request = GatewayRequest {
            run_id: self.identity.execution_id.clone(),
            step_id: format!("step_{step_count}"),
            transcript: self.state.transcript.to_vec(),
            system_prompt: self
                .request
                .context
                .system_prompt
                .clone()
                .unwrap_or(agent.system_prompt),
            model: model.id.clone(),
            provider: model.provider.clone(),
            tools,
            thinking: None,
        };

        // Bridge: StepDispatch identity still uses task_id; pass execution_id.
        let identity = DispatchIdentity::new(
            self.identity.session_id.clone(),
            self.identity.execution_id.clone(),
            self.identity.agent_id.clone(),
        );

        let result = match self
            .services
            .model_executor()
            .chat_stream(request, Some(self.cancel.clone()))
            .await
        {
            Ok(llm) => {
                let mut dispatch = StepDispatch::from_step_stream(
                    identity,
                    message_id.clone(),
                    self.identity.execution_id.clone(),
                    model,
                    llm,
                );
                Ok(dispatch.dispatch_step().await)
            }
            Err(error) => {
                if self.cancel.is_cancelled() {
                    return Err(AgentApiError::Cancelled);
                }
                let mut dispatch = StepDispatch::from_step_failure(
                    identity,
                    message_id.clone(),
                    self.identity.execution_id.clone(),
                    model,
                    error.to_string(),
                );
                let result = dispatch.dispatch_step().await;
                Err((error.to_string(), result))
            }
        };

        match result {
            Ok(step) => {
                self.publish_snapshot();
                Ok(CompletedModelStep {
                    assistant_message: step.step.assistant_message,
                    tool_calls: step.step.tool_calls,
                    routes,
                    message_id,
                })
            }
            Err((error, step)) => {
                if !matches!(&step.step.assistant_message, Message::Assistant { .. }) {
                    return Err(AgentApiError::PersistenceFailed(error));
                }
                self.commit_assistant(&step.step.assistant_message, &message_id)
                    .await?;
                Err(AgentApiError::PersistenceFailed(error))
            }
        }
    }

    async fn execute_and_commit_tools(
        &mut self,
        tool_calls: &[ToolCallItem],
        routes: &HashMap<String, CatalogRoute>,
        parent_message_id: &str,
    ) -> Result<(), AgentApiError> {
        for tc in tool_calls {
            if self.cancel.is_cancelled() {
                return Ok(());
            }

            let tool_call_message = Message::ToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
                model: None,
                provider: None,
                timestamp: Some(chrono::Utc::now().timestamp_millis()),
            };
            let tool_call_message_id =
                format!("{}:tool_call:{}", parent_message_id, tc.tool_call_index);
            self.commit_message(&tool_call_message, &tool_call_message_id)
                .await?;
            self.state.transcript.push_message(tool_call_message);

            let result_message = match routes.get(&tc.name) {
                Some(route) => {
                    let call = ToolCall {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                        partial_json: None,
                    };
                    let exec_ctx = ToolExecutionContext {
                        agent_id: self.identity.agent_id.clone(),
                        task_id: self.identity.execution_id.clone(),
                        tool_set_ids: vec![],
                        turn_index: Some(self.state.model_step_index),
                        event_seq: Some(0),
                        next_event_seq: None,
                        parent_message_id: Some(parent_message_id.to_string()),
                        content_index: Some(tc.content_index),
                        tool_call_index: Some(tc.tool_call_index),
                        tool_entity_id: Some(runtime_tool_entity_id(
                            parent_message_id,
                            tc.tool_call_index,
                        )),
                        host_context: Some(piko_protocol::agents::HostTaskContext::new(
                            self.identity.session_id.clone(),
                        )),
                        active_work_id: Some(self.identity.execution_id.clone()),
                        source_turn_id: Some(self.identity.turn_id.clone()),
                    };
                    let record = (*self.services.tool_registry())
                        .execute_tool(&call, &exec_ctx, route, Some(self.cancel.clone()))
                        .await;
                    append_tool(&mut self.state.transcript, tc, &record.result)
                }
                None => append_tool_err(
                    &mut self.state.transcript,
                    tc,
                    &format!("No route for tool \"{}\"", tc.name),
                ),
            };

            // append_tool already pushed to transcript; pop and re-commit with identity.
            let result_message_id =
                format!("{}:tool_result:{}", parent_message_id, tc.tool_call_index);
            // transcript already has the message from append_*; commit durable copy.
            self.commit_message(&result_message, &result_message_id)
                .await?;
            // append_* already pushed; avoid double push by not pushing again.
            // But commit_message doesn't push — append already did. Good.
            // Wait: append_tool pushes then returns the message. We commit that. Transcript already has it. OK.
            let _ = result_message;
        }
        self.publish_snapshot();
        Ok(())
    }

    async fn resolve_model(&self, agent: &piko_protocol::agents::AgentSpec) -> ModelSpec {
        if let Some(model) = self.services.model_config().await {
            return model.model;
        }
        ModelSpec {
            id: self
                .request
                .config
                .model
                .clone()
                .or_else(|| agent.model.clone())
                .unwrap_or_else(|| "default".into()),
            name: "default".into(),
            provider: self
                .request
                .config
                .provider
                .clone()
                .unwrap_or_else(|| "default".into()),
        }
    }

    async fn commit_assistant(
        &mut self,
        message: &Message,
        message_id: &str,
    ) -> Result<(), AgentApiError> {
        self.commit_message(message, message_id).await
    }

    async fn commit_message(
        &mut self,
        message: &Message,
        message_id: &str,
    ) -> Result<(), AgentApiError> {
        let commit = piko_protocol::execution::MessageCommit {
            session_id: self.identity.session_id.clone(),
            turn_id: self.identity.turn_id.clone(),
            execution_id: self.identity.execution_id.clone(),
            message_id: message_id.to_string(),
            parent_message_id: self.state.head_message_id.clone(),
            message: message.clone(),
            committed_at: chrono::Utc::now().timestamp_millis(),
        };
        self.ports
            .ports()
            .commit
            .commit_message(commit)
            .await
            .map_err(|err| AgentApiError::PersistenceFailed(err.to_string()))?;
        self.state.head_message_id = Some(message_id.to_string());
        self.publish_snapshot();
        Ok(())
    }

    async fn commit_steering(
        &mut self,
        steering: &SteerExecutionRequest,
    ) -> Result<(), AgentApiError> {
        let message = Message::User {
            content: steering.content.clone(),
            timestamp: Some(steering.submitted_at),
        };
        self.commit_message(&message, &steering.message_id).await?;
        self.state.transcript.push_message(message);
        Ok(())
    }
}

struct CompletedModelStep {
    assistant_message: Message,
    tool_calls: Vec<ToolCallItem>,
    routes: HashMap<String, CatalogRoute>,
    message_id: String,
}
