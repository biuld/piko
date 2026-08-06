use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use piko_comms::MailboxReceiver;
use piko_comms::contracts::ExecutionCommands;
use piko_orchd_api::telemetry::ModelStepTelemetry;
use piko_orchd_api::{AgentApiError, CancelReceipt, InputDisposition};
use piko_protocol::execution::ExecutionInputReceipt;
use piko_protocol::execution::{
    ExecutionOutcome, ExecutionStatus, StartExecutionRequest, SteerExecutionRequest,
};
use piko_protocol::{Message, Usage};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::ExecutionIdentity;
use super::mailbox::ExecutionCommand;
use super::scope::SessionExecutionScope;
use super::services::ExecutionServices;
use super::state::ExecutionState;
use super::tool_batch;
use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::model::step::ModelSpec;
use crate::domain::tools::call::ToolCallItem;
use crate::domain::tools::definition::ToolExecutionMode;
use crate::domain::transcript::{TranscriptManager, TranscriptPolicy};
use crate::runtime::events::identity::DispatchIdentity;
use crate::runtime::reliability::{ActorCommandScope, MessageCommitScope};
use crate::runtime::runtime_assistant_message_id;
use crate::runtime::step::StepDispatch;
use crate::runtime::tools::{build_tool_error, build_tool_result};
use piko_llmd::gateway::GatewayRequest;

#[derive(Debug, Clone)]
pub struct ExecutionRunResult {
    pub outcome: ExecutionOutcome,
    pub transcript: Vec<Message>,
    pub head_message_id: Option<String>,
}

pub struct ExecutionActor {
    identity: ExecutionIdentity,
    state: ExecutionState,
    mailbox: MailboxReceiver<ExecutionCommands, ExecutionCommand>,
    cancel: CancellationToken,
    ports: Arc<SessionExecutionScope>,
    services: ExecutionServices,
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
        mailbox: MailboxReceiver<ExecutionCommands, ExecutionCommand>,
        cancel: CancellationToken,
        ports: Arc<SessionExecutionScope>,
        services: ExecutionServices,
    ) -> Self {
        let mut transcript = TranscriptManager::new(Some(request.context.messages.clone()));
        if let Some(world_state) = &request.world_state {
            // Retained world-state (F-04 slice 2): committed by the startup
            // scope before the input commit, so the in-memory order matches
            // the durable linear chain head → world-state → input.
            transcript.push_message(world_state.clone());
        }
        // Inter-agent completions (F-20): same commit path as world-state,
        // after world-state and before the run input.
        for completion in &request.inter_agent_completions {
            transcript.push_message(completion.clone());
        }
        // File/skill mentions (F-03 / D-27): after inter-agent completions,
        // before the user input on the durable chain.
        for mention in &request.user_mentions {
            transcript.push_message(mention.clone());
        }
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
        // Interrupted turns append a durable, model-visible abort marker
        // before the terminal, so the next run can see that work may have
        // partially executed (F-01 / D-01). A marker commit failure fails
        // closed rather than silently dropping the abort.
        let outcome = match outcome {
            ExecutionOutcome::Cancelled { .. } => match self.commit_abort_marker().await {
                Ok(()) => outcome,
                Err(error) => {
                    ExecutionOutcome::failed(format!("abort marker commit failed: {error}"))
                }
            },
            other => other,
        };
        ExecutionRunResult {
            outcome,
            transcript: self.state.transcript.to_vec(),
            head_message_id: self.state.head_message_id.clone(),
        }
    }

    async fn commit_abort_marker(&mut self) -> Result<(), AgentApiError> {
        let message = piko_protocol::turn_abort_marker(&self.identity.execution_id);
        let message_id = piko_protocol::turn_abort_marker_message_id(&self.identity.execution_id);
        self.commit_message(message, message_id).await
    }

    async fn run_loop(&mut self) -> Result<ExecutionOutcome, AgentApiError> {
        self.transition(ExecutionStatus::Running);

        loop {
            if self.cancel.is_cancelled() {
                return Ok(ExecutionOutcome::Cancelled {
                    reason: Some("cancelled".into()),
                });
            }

            self.drain_controls_nonblocking()?;

            let step_span = tracing::info_span!(
                "model.step",
                session_id = %self.identity.session_id,
                run_id = %self.identity.execution_id,
                agent_instance_id = %self.identity.agent_instance_id,
                step_id = tracing::field::Empty,
                model = tracing::field::Empty,
                provider = tracing::field::Empty,
                thinking = tracing::field::Empty,
                tools = tracing::field::Empty,
                transcript_messages = tracing::field::Empty,
                transcript_tokens = tracing::field::Empty,
                truncated_outputs = tracing::field::Empty,
                context_window = tracing::field::Empty,
                context_remaining = tracing::field::Empty,
            );
            let step_started = std::time::Instant::now();
            let iteration = async {
                let step = self.run_model_step().await?;
                self.commit_message(step.assistant_message, step.message_id.clone())
                    .await?;

                if !step.tool_calls.is_empty() {
                    if !self.request.config.allow_tool_calls {
                        return Err(AgentApiError::InputRejected);
                    }
                    self.execute_and_commit_tools(
                        &step.tool_calls,
                        &step.routes,
                        &step.message_id,
                        step.context_remaining,
                    )
                    .await?;
                    self.drain_controls_at_step_boundary().await?;
                    if let Some(steering) = self.state.steering.pop_front() {
                        self.commit_steering(&steering).await?;
                    }
                    return Ok((true, step.model));
                }

                self.drain_controls_at_step_boundary().await?;
                if let Some(steering) = self.state.steering.pop_front() {
                    self.commit_steering(&steering).await?;
                    return Ok((true, step.model));
                }

                Ok((false, step.model))
            }
            .instrument(step_span)
            .await?;

            let (more, model) = iteration;
            self.services
                .telemetry()
                .model_step_completed(ModelStepTelemetry {
                    model: model.id,
                    provider: model.provider,
                    duration_ms: step_started.elapsed().as_millis() as u64,
                    status: "ok",
                });
            if more {
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

        let model_config = self.services.model_config().await;
        let model = model_config
            .as_ref()
            .map(|config| config.model.clone())
            .unwrap_or_else(|| self.resolve_fallback_model(&agent));
        let thinking = if let Some(level) = agent.thinking_level.as_ref() {
            model_config.as_ref().and_then(|config| {
                config
                    .thinking_level_map
                    .as_ref()
                    .and_then(|map| map.get(level).cloned())
                    .unwrap_or_else(|| Some(level.as_str().to_string()))
            })
        } else {
            model_config
                .as_ref()
                .and_then(|config| config.resolve_thinking())
        };
        let (tools, routes) = if self.request.config.allow_tool_calls {
            (self.tools.clone(), self.routes.clone())
        } else {
            (Vec::new(), HashMap::new())
        };
        let max_tool_output_tokens = model_config
            .as_ref()
            .map(|config| config.max_tool_output_tokens)
            .unwrap_or(TranscriptPolicy::default().max_tool_output_tokens);
        let model_view = self.state.transcript.model_view(&TranscriptPolicy {
            max_tool_output_tokens,
        });
        let snapshot = &model_view.snapshot;
        let transcript = snapshot.messages().to_vec();
        let mut context_remaining = None;
        let span = tracing::Span::current();
        span.record("step_id", format!("step_{step_count}"));
        span.record("model", &model.id);
        span.record("provider", &model.provider);
        span.record("thinking", thinking.as_deref().unwrap_or("none"));
        span.record("tools", tools.len());
        span.record("transcript_messages", transcript.len());
        span.record("transcript_tokens", snapshot.total_tokens());
        span.record("truncated_outputs", model_view.truncated_outputs);
        if let Some(config) = model_config.as_ref() {
            span.record("context_window", config.context_window);
            let estimate = super::budget::enforce_context_budget(
                &self.request.run_prompt,
                snapshot,
                &tools,
                config.context_window,
                config.max_output_tokens,
                thinking.is_some(),
            )?;
            span.record("context_remaining", estimate.context_remaining);
            context_remaining = Some(estimate.context_remaining);
        }

        let request = GatewayRequest {
            session_id: self.identity.session_id.clone(),
            agent_instance_id: self.identity.agent_instance_id.clone(),
            run_id: self.identity.execution_id.clone(),
            step_id: format!("step_{step_count}"),
            transcript,
            run_prompt: self.request.run_prompt.clone(),
            model: model.id.clone(),
            provider: model.provider.clone(),
            tools,
            thinking,
        };
        tracing::debug!(
            execution_id = %self.identity.execution_id,
            step_id = %request.step_id,
            prompt_assembly_version = request.run_prompt.assembly_version,
            prompt_source_digest = %request.run_prompt.source_digest,
            prompt_prefix_digest = %request.run_prompt.cache_plan.semantic_prefix_digest,
            prompt_blocks = request.run_prompt.blocks.len(),
            tools = request.tools.len(),
            transcript_messages = request.transcript.len(),
            "dispatching semantic model request"
        );

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
                    model.clone(),
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
                    model.clone(),
                    error.to_string(),
                );
                let result = dispatch
                    .dispatch_step(self.ports.ports().realtime.clone())
                    .await;
                Err((error.to_string(), result))
            }
        };

        match result {
            Ok(step) => Ok(CompletedModelStep {
                assistant_message: step.step.assistant_message,
                tool_calls: step.step.tool_calls,
                routes,
                message_id,
                model,
                context_remaining,
            }),
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
        context_remaining: Option<u64>,
    ) -> Result<(), AgentApiError> {
        // Batch dispatch groups consecutive calls by their effective execution
        // mode (F-06 / D-06): parallel calls in a group overlap under a shared
        // cap, sequential calls run exclusively, and results commit in
        // tool_call_index order so the append-only transcript stays
        // deterministic per run.
        let registry = self.services.tool_registry().clone();
        let model_step_index = self.state.model_step_index;
        let mut fresh_window_requested = false;
        for group in tool_batch::group_tool_calls(tool_calls, routes) {
            let batch_span = tracing::info_span!(
                "tool.batch",
                session_id = %self.identity.session_id,
                run_id = %self.identity.execution_id,
                agent_instance_id = %self.identity.agent_instance_id,
                step_id = format!("step_{model_step_index}"),
                mode = tool_batch::mode_str(&group.mode),
                call_count = group.calls.len(),
                concurrency_cap = tracing::field::Empty,
                tool_names = group
                    .calls
                    .iter()
                    .map(|tc| tc.name.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            );
            let telemetry = self.services.telemetry();
            let batch_result: Result<(), AgentApiError> = async {
                match group.mode {
                    ToolExecutionMode::Sequential => {
                        for tc in group.calls {
                            self.commit_message(
                                tool_batch::tool_call_message(tc),
                                tool_batch::tool_call_message_id(
                                    parent_message_id,
                                    tc.tool_call_index,
                                ),
                            )
                            .await?;

                            let record = if self.cancel.is_cancelled() {
                                Some(tool_batch::aborted_tool_exec_result())
                            } else {
                                match routes.get(&tc.name) {
                                    Some(route) => Some(
                                        tool_batch::execute_sequential_call(
                                            registry.clone(),
                                            self.cancel.clone(),
                                            model_step_index,
                                            &self.identity,
                                            tc,
                                            route,
                                            parent_message_id,
                                            context_remaining,
                                            Arc::clone(&telemetry),
                                        )
                                        .await,
                                    ),
                                    None => {
                                        Some(tool_batch::no_route_error(&registry, &tc.name).await)
                                    }
                                }
                            };
                            let result_message = match record {
                                Some(ref record) => build_tool_result(tc, record),
                                None => build_tool_error(
                                    tc,
                                    &format!("No route for tool \"{}\"", tc.name),
                                ),
                            };
                            if tc.name == "new_context_window"
                                && record.as_ref().is_some_and(|result| result.ok)
                            {
                                fresh_window_requested = true;
                            }
                            self.commit_message(
                                result_message,
                                tool_batch::tool_result_message_id(
                                    parent_message_id,
                                    tc.tool_call_index,
                                ),
                            )
                            .await?;
                        }
                    }
                    ToolExecutionMode::Parallel => {
                        for tc in &group.calls {
                            self.commit_message(
                                tool_batch::tool_call_message(tc),
                                tool_batch::tool_call_message_id(
                                    parent_message_id,
                                    tc.tool_call_index,
                                ),
                            )
                            .await?;
                        }
                        let results = tool_batch::execute_parallel_group(
                            registry.clone(),
                            self.cancel.clone(),
                            model_step_index,
                            &self.identity,
                            &group.calls,
                            routes,
                            parent_message_id,
                            context_remaining,
                            Arc::clone(&telemetry),
                        )
                        .await;
                        for (tc, result) in group.calls.iter().zip(results) {
                            if tc.name == "new_context_window" && result.ok {
                                fresh_window_requested = true;
                            }
                            self.commit_message(
                                build_tool_result(tc, &result),
                                tool_batch::tool_result_message_id(
                                    parent_message_id,
                                    tc.tool_call_index,
                                ),
                            )
                            .await?;
                        }
                    }
                }
                Ok(())
            }
            .instrument(batch_span)
            .await;
            let _ = batch_result;
        }
        if fresh_window_requested {
            // The model asked for a fresh window: the durable hostd tree was
            // rewritten through the callback; keep the running execution
            // aligned by dropping everything before the latest user message.
            self.state.transcript.reset_to_recent_user();
        }
        Ok(())
    }

    fn resolve_fallback_model(&self, agent: &piko_protocol::agents::AgentSpec) -> ModelSpec {
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

    #[cfg(test)]
    pub(crate) fn transcript_messages(&self) -> Vec<Message> {
        self.state.transcript.to_vec()
    }

    async fn commit_message(
        &mut self,
        message: Message,
        message_id: String,
    ) -> Result<(), AgentApiError> {
        if let Message::Assistant {
            usage: Some(usage), ..
        } = &message
        {
            self.state.usage.accumulate(usage);
        }
        let committed = MessageCommitScope::new(
            &self.identity,
            self.state.head_message_id.clone(),
            message_id,
            message,
        )
        .commit(&self.ports.ports().commit)
        .await?;
        committed.apply(&mut self.state);
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
    model: ModelSpec,
    context_remaining: Option<u64>,
}
