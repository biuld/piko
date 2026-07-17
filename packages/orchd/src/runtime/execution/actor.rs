use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use orchd_api::{AgentApiError, CancelReceipt, InputDisposition};
use piko_protocol::execution::ExecutionInputReceipt;
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
use crate::ports::tool_provider::ToolExecutionContext;
use crate::runtime::events::identity::DispatchIdentity;
use crate::runtime::reliability::{ActorCommandScope, MessageCommitScope};
use crate::runtime::runtime_assistant_message_id;
use crate::runtime::step::StepDispatch;
use crate::runtime::tools::{build_tool_error, build_tool_result};
use crate::runtime::utils::runtime_tool_entity_id;
use llmd::gateway::GatewayRequest;

#[derive(Debug, Clone)]
pub struct ExecutionRunResult {
    pub outcome: ExecutionOutcome,
    pub transcript: Vec<Message>,
    pub head_message_id: Option<String>,
}

pub struct ExecutionActor {
    identity: ExecutionIdentity,
    state: ExecutionState,
    mailbox: mpsc::Receiver<ExecutionCommand>,
    cancel: CancellationToken,
    ports: Arc<SessionExecutionScope>,
    services: ExecutionServices,
    snapshot_tx: watch::Sender<ExecutionSnapshot>,
    request: StartExecutionRequest,
    tools: Vec<piko_protocol::ToolDef>,
    routes: HashMap<String, CatalogRoute>,
}

impl ExecutionActor {
    pub fn new(
        identity: ExecutionIdentity,
        request: StartExecutionRequest,
        tools: Vec<piko_protocol::ToolDef>,
        routes: HashMap<String, CatalogRoute>,
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
            // PreparedExecution commits the input before activation, so the
            // first live transcript head is always durable.
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
            tools,
            routes,
        }
    }

    pub fn identity(&self) -> &ExecutionIdentity {
        &self.identity
    }

    pub async fn run(mut self) -> ExecutionRunResult {
        let outcome = match self.run_loop().await {
            Ok(outcome) => outcome,
            Err(AgentApiError::Cancelled) => ExecutionOutcome::Cancelled {
                reason: Some("cancelled".into()),
            },
            Err(error) => ExecutionOutcome::failed(error.to_string()),
        };
        ExecutionRunResult {
            outcome,
            transcript: self.state.transcript.to_vec(),
            head_message_id: self.state.head_message_id.clone(),
        }
    }

    async fn run_loop(&mut self) -> Result<ExecutionOutcome, AgentApiError> {
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
            self.commit_message(step.assistant_message, step.message_id.clone())
                .await?;

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
            source_turn_id: self.identity.source_turn_id.clone(),
            execution_id: self.identity.execution_id.clone(),
            agent_instance_id: self.identity.agent_instance_id.clone(),
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
                let command = ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                let receipt = ExecutionInputReceipt {
                    request_id: request.request_id.clone(),
                    session_id: self.identity.session_id.clone(),
                    execution_id: self.identity.execution_id.clone(),
                    message_id: request.message_id.clone(),
                    disposition: InputDisposition::Queued,
                };
                self.state.steering.push_back(request);
                command.complete(Ok(receipt));
            }
            ExecutionCommand::Cancel {
                request_id,
                reason: _,
                reply,
            } => {
                let command = ActorCommandScope::new(reply, Err(AgentApiError::RuntimeUnavailable));
                self.cancel.cancel();
                command.complete(Ok(CancelReceipt {
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

        let agent = self.request.agent_spec.clone();

        let model = self.resolve_model(&agent).await;
        let (tools, routes) = if self.request.config.allow_tool_calls {
            (self.tools.clone(), self.routes.clone())
        } else {
            (Vec::new(), HashMap::new())
        };

        let request = GatewayRequest {
            run_id: self.identity.execution_id.clone(),
            step_id: format!("step_{step_count}"),
            transcript: self.state.transcript.to_vec(),
            system_prompt: self.request.run_prompt.system_prompt.clone(),
            model: model.id.clone(),
            provider: model.provider.clone(),
            tools,
            thinking: None,
        };

        // Pass Interaction Turn binding into StepDispatch (empty for child runs).
        let identity = DispatchIdentity::new(
            self.identity.session_id.clone(),
            self.identity.agent_instance_id.clone(),
            self.identity.execution_id.clone(),
            self.identity.agent_id.clone(),
        );
        let source_turn_id = self.identity.source_turn_id.clone().unwrap_or_default();

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
                    source_turn_id.clone(),
                    model,
                    llm,
                );
                Ok(dispatch
                    .dispatch_step(self.ports.ports().realtime.clone())
                    .await)
            }
            Err(error) => {
                if self.cancel.is_cancelled() {
                    return Err(AgentApiError::Cancelled);
                }
                let mut dispatch = StepDispatch::from_step_failure(
                    identity,
                    message_id.clone(),
                    source_turn_id,
                    model,
                    error.to_string(),
                );
                let result = dispatch
                    .dispatch_step(self.ports.ports().realtime.clone())
                    .await;
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
                self.commit_message(step.step.assistant_message, message_id)
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
            self.commit_message(tool_call_message, tool_call_message_id)
                .await?;

            let result_message = match routes.get(&tc.name) {
                Some(route) => {
                    let call = ToolCall {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                        partial_json: None,
                    };
                    let exec_ctx = ToolExecutionContext {
                        session_id: self.identity.session_id.clone(),
                        agent_instance_id: self.identity.agent_instance_id.clone(),
                        execution_id: self.identity.execution_id.clone(),
                        cancellation: Some(self.cancel.clone()),
                        agent_id: self.identity.agent_id.clone(),
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
                        host_context: Some(piko_protocol::agents::HostSessionContext::new(
                            self.identity.session_id.clone(),
                        )),
                        source_turn_id: self.identity.source_turn_id.clone(),
                    };
                    let record = (*self.services.tool_registry())
                        .execute_tool(&call, &exec_ctx, route, Some(self.cancel.clone()))
                        .await;
                    build_tool_result(tc, &record.result)
                }
                None => build_tool_error(tc, &format!("No route for tool \"{}\"", tc.name)),
            };

            let result_message_id =
                format!("{}:tool_result:{}", parent_message_id, tc.tool_call_index);
            self.commit_message(result_message, result_message_id)
                .await?;
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

    async fn commit_message(
        &mut self,
        message: Message,
        message_id: String,
    ) -> Result<(), AgentApiError> {
        let committed = MessageCommitScope::new(
            &self.identity,
            self.state.head_message_id.clone(),
            message_id,
            message,
        )
        .commit(&self.ports.ports().commit)
        .await?;
        committed.apply(&mut self.state);
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
        self.commit_message(message, steering.message_id.clone())
            .await?;
        Ok(())
    }
}

struct CompletedModelStep {
    assistant_message: Message,
    tool_calls: Vec<ToolCallItem>,
    routes: HashMap<String, CatalogRoute>,
    message_id: String,
}
