use super::*;

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

    pub(super) async fn commit_abort_marker(&mut self) -> Result<(), AgentApiError> {
        let message = piko_protocol::turn_abort_marker(&self.identity.execution_id);
        let message_id = piko_protocol::turn_abort_marker_message_id(&self.identity.execution_id);
        self.commit_message(message, message_id).await
    }

    pub(super) async fn run_loop(&mut self) -> Result<ExecutionOutcome, AgentApiError> {
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

    pub(super) fn transition(&mut self, status: ExecutionStatus) {
        self.state.status = status;
    }

    pub(super) fn drain_controls_nonblocking(&mut self) -> Result<(), AgentApiError> {
        while let Ok(command) = self.mailbox.try_recv() {
            self.handle_command(command)?;
        }
        Ok(())
    }

    pub(super) async fn drain_controls_at_step_boundary(&mut self) -> Result<(), AgentApiError> {
        self.drain_controls_nonblocking()?;
        Ok(())
    }

    pub(super) fn handle_command(
        &mut self,
        command: ExecutionCommand,
    ) -> Result<(), AgentApiError> {
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

    pub(super) async fn run_model_step(&mut self) -> Result<CompletedModelStep, AgentApiError> {
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
        let thinking = agent.thinking_level.clone().or_else(|| {
            model_config
                .as_ref()
                .and_then(|config| config.settings.thinking_level.clone())
        });
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
        span.record(
            "thinking",
            thinking
                .as_ref()
                .map(|value| value.as_str())
                .unwrap_or("none"),
        );
        span.record("tools", tools.len());
        span.record("transcript_messages", transcript.len());
        span.record("transcript_tokens", snapshot.total_tokens());
        span.record("truncated_outputs", model_view.truncated_outputs);
        if let Some(config) = model_config.as_ref() {
            span.record("context_window", config.context_window);
            let estimate = super::super::budget::enforce_context_budget(
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

        let request = InferenceRequest {
            model: piko_llmd::gateway::ModelRef::new(&model.provider, &model.id),
            conversation: piko_llmd::gateway::Conversation::from_messages(
                self.request.run_prompt.clone(),
                transcript,
            ),
            tools: tools.into_iter().map(Into::into).collect(),
            options: piko_llmd::gateway::InferenceOptions {
                reasoning_effort: thinking,
                tool_choice: model_config
                    .as_ref()
                    .and_then(|config| config.settings.tool_choice.as_ref())
                    .map(|choice| match choice {
                        piko_protocol::model::ModelToolChoice::Auto => {
                            piko_llmd::gateway::ToolChoice::Auto
                        }
                        piko_protocol::model::ModelToolChoice::None => {
                            piko_llmd::gateway::ToolChoice::None
                        }
                        piko_protocol::model::ModelToolChoice::Required => {
                            piko_llmd::gateway::ToolChoice::Required
                        }
                        piko_protocol::model::ModelToolChoice::Specific { name } => {
                            piko_llmd::gateway::ToolChoice::Specific(name.clone())
                        }
                    })
                    .unwrap_or_default(),
                parallel_tools: model_config
                    .as_ref()
                    .and_then(|config| config.settings.parallel_tools),
                max_output_tokens: model_config
                    .as_ref()
                    .and_then(|config| config.settings.max_tokens),
                ..Default::default()
            },
            context: piko_llmd::gateway::InvocationContext {
                session_id: self.identity.session_id.clone(),
                agent_instance_id: self.identity.agent_instance_id.clone(),
                run_id: self.identity.execution_id.clone(),
                step_id: format!("step_{step_count}"),
            },
        };
        tracing::debug!(
            execution_id = %self.identity.execution_id,
            step_id = %request.context.step_id,
            prompt_assembly_version = request.conversation.instructions.assembly_version,
            prompt_source_digest = %request.conversation.instructions.source_digest,
            prompt_prefix_digest = %request.conversation.instructions.cache_plan.semantic_prefix_digest,
            prompt_blocks = request.conversation.instructions.blocks.len(),
            tools = request.tools.len(),
            transcript_messages = request.conversation.items.len(),
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
            .start(request, self.cancel.clone())
            .await
        {
            Ok(llm) => {
                let mut dispatch = StepDispatch::from_step_stream(
                    identity,
                    message_id.clone(),
                    source_turn_id.clone(),
                    model.clone(),
                    llm.events,
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
}
