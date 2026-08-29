use super::*;

impl AgentExecutionRuntime {
    pub(crate) async fn attach_session(
        &self,
        session_id: String,
        ports: SessionExecutionPorts,
    ) -> Result<(), AgentApiError> {
        if !self.accepting.load(Ordering::SeqCst) {
            return Err(AgentApiError::RuntimeUnavailable);
        }
        let mut sessions = self.sessions.write().await;
        if sessions.contains_key(&session_id) {
            return Err(AgentApiError::SessionAlreadyAttached);
        }
        sessions.insert(
            session_id.clone(),
            Arc::new(SessionExecutionScope::new(session_id, ports)),
        );
        Ok(())
    }

    pub(crate) async fn detach_session(&self, session_id: String) -> Result<(), AgentApiError> {
        let scope = {
            let mut sessions = self.sessions.write().await;
            sessions
                .remove(&session_id)
                .ok_or(AgentApiError::SessionNotAttached)?
        };
        scope.cancel_all().await;
        if scope.drain().await {
            Ok(())
        } else {
            Err(AgentApiError::RuntimeUnavailable)
        }
    }

    pub(crate) async fn prepare_execution(
        &self,
        request: StartExecutionRequest,
        routes: HashMap<String, CatalogRoute>,
        trace_span: tracing::Span,
    ) -> Result<PreparedExecution, AgentApiError> {
        if !self.accepting.load(Ordering::SeqCst) {
            return Err(AgentApiError::RuntimeUnavailable);
        }
        let scope = self.scope(&request.session_id).await?;
        let generation = scope.next_generation();
        let cancel = CancellationToken::new();
        let (command_tx, command_rx) = piko_comms::mailbox::<ExecutionCommands, _>();
        let (terminal_tx, terminal_rx) = piko_comms::reply::<ExecutionTerminalContract, _>();

        // F-19: resolve the executing agent's role from the registered spec
        // (identity metadata; hostd maps it to a permission profile).
        let agent_role = self
            .services
            .agent_spec(&request.config.agent_id)
            .await
            .map(|spec| spec.role);
        let identity = ExecutionIdentity {
            session_id: request.session_id.clone(),
            source_turn_id: request.source_turn_id.clone(),
            run_id: request.request_id.clone(),
            execution_id: request.execution_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            agent_id: request.config.agent_id.clone(),
            agent_role,
            agent_kind: request.agent_spec.kind,
        };

        let now_ms = chrono::Utc::now().timestamp_millis();
        let world_state_commit =
            request
                .world_state
                .as_ref()
                .map(|message| piko_protocol::execution::MessageCommit {
                    session_id: request.session_id.clone(),
                    source_turn_id: request.source_turn_id.clone(),
                    execution_id: request.execution_id.clone(),
                    agent_instance_id: request.agent_instance_id.clone(),
                    message_id: piko_protocol::world_state_message_id(&request.execution_id),
                    parent_message_id: request.context.head_message_id.clone(),
                    tree_parent_entry_id: None,
                    message: message.clone(),
                    committed_at: now_ms,
                });
        // Linear durable chain (hostd enforces parent == head):
        // head → world-state? → inter-agent completions… → input.
        let mut chain_parent = world_state_commit
            .as_ref()
            .map(|commit| commit.message_id.clone())
            .or_else(|| request.context.head_message_id.clone());
        let mut completion_commits = Vec::with_capacity(request.inter_agent_completions.len());
        for message in &request.inter_agent_completions {
            let message_id = match message {
                piko_protocol::Message::Context { source, .. }
                    if source.kind == piko_protocol::AGENT_COMPLETION_SOURCE_KIND =>
                {
                    piko_protocol::agent_completion_message_id(&source.locator)
                }
                _ => {
                    return Err(AgentApiError::InputRejected);
                }
            };
            let commit = piko_protocol::execution::MessageCommit {
                session_id: request.session_id.clone(),
                source_turn_id: request.source_turn_id.clone(),
                execution_id: request.execution_id.clone(),
                agent_instance_id: request.agent_instance_id.clone(),
                message_id: message_id.clone(),
                parent_message_id: chain_parent.clone(),
                tree_parent_entry_id: None,
                message: message.clone(),
                committed_at: now_ms,
            };
            chain_parent = Some(message_id);
            completion_commits.push(commit);
        }
        let mut mention_commits = Vec::with_capacity(request.user_mentions.len());
        for (index, message) in request.user_mentions.iter().enumerate() {
            let message_id = match message {
                piko_protocol::Message::Context { source, .. }
                    if source.kind == piko_protocol::FILE_MENTION_SOURCE_KIND =>
                {
                    piko_protocol::file_mention_message_id(&request.execution_id, index)
                }
                piko_protocol::Message::Context { source, .. }
                    if source.kind == piko_protocol::SKILL_MENTION_SOURCE_KIND =>
                {
                    piko_protocol::skill_mention_message_id(&request.execution_id, index)
                }
                _ => {
                    return Err(AgentApiError::InputRejected);
                }
            };
            let commit = piko_protocol::execution::MessageCommit {
                session_id: request.session_id.clone(),
                source_turn_id: request.source_turn_id.clone(),
                execution_id: request.execution_id.clone(),
                agent_instance_id: request.agent_instance_id.clone(),
                message_id: message_id.clone(),
                parent_message_id: chain_parent.clone(),
                tree_parent_entry_id: None,
                message: message.clone(),
                committed_at: now_ms,
            };
            chain_parent = Some(message_id);
            mention_commits.push(commit);
        }
        let input_commit = piko_protocol::execution::MessageCommit {
            session_id: request.session_id.clone(),
            source_turn_id: request.source_turn_id.clone(),
            execution_id: request.execution_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            message_id: request.input_message_id.clone(),
            parent_message_id: chain_parent,
            tree_parent_entry_id: None,
            message: piko_protocol::Message::User {
                content: request.input.clone(),
                timestamp: Some(now_ms),
            },
            committed_at: now_ms,
        };

        let handle = ExecutionHandle {
            identity: identity.clone(),
            generation,
            command_tx,
            cancel: cancel.clone(),
            terminal_rx: crate::runtime::execution::mailbox::ArcTerminalReceiver::new(terminal_rx),
        };

        scope.reserve_execution(handle.clone()).await?;

        let receipt = ExecutionReceipt {
            request_id: request.request_id.clone(),
            session_id: identity.session_id.clone(),
            source_turn_id: identity.source_turn_id.clone(),
            execution_id: identity.execution_id.clone(),
            agent_instance_id: identity.agent_instance_id.clone(),
            status: ExecutionStatus::Accepted,
        };

        let tools = request.tool_catalog.tools.clone();
        let actor = ExecutionActor::new(
            identity,
            request,
            tools,
            routes,
            command_rx,
            cancel,
            Arc::clone(&scope),
            self.services.clone(),
        );

        Ok(PreparedExecution {
            scope,
            actor: Some(actor),
            generation,
            terminal_tx: Some(terminal_tx),
            receipt,
            world_state_commit,
            completion_commits,
            mention_commits,
            input_commit,
            trace_span,
        })
    }

    pub(crate) async fn prepare_run_context(
        &self,
        request: &piko_protocol::SendAgentInputRequest,
        agent_spec: &AgentSpec,
        run_id: &str,
    ) -> Result<PreparedRunContext, AgentApiError> {
        let active_tool_names = match (
            agent_spec.active_tool_names.as_ref(),
            request.active_tool_names.as_ref(),
        ) {
            (Some(stable), Some(transient)) => Some(
                stable
                    .iter()
                    .filter(|name| transient.contains(name))
                    .cloned()
                    .collect(),
            ),
            (Some(stable), None) => Some(stable.clone()),
            (None, Some(transient)) => Some(transient.clone()),
            (None, None) => None,
        };
        let (tools, routes) = self
            .services
            .tool_registry()
            .discover_tools(&ToolDiscoveryContext {
                agent_id: agent_spec.id.clone(),
                agent_kind: agent_spec.kind,
                agent_instance_id: Some(request.agent_instance_id.clone()),
                tool_set_ids: agent_spec.tool_set_ids.clone(),
                active_tool_names,
            })
            .await
            .map_err(AgentApiError::ToolCatalogFailed)?;
        let scope = self.scope(&request.session_id).await?;
        let tool_catalog = prompt::resolved_tool_catalog(tools.clone());
        let frozen_catalog = tool_catalog.clone();
        let assembly = piko_protocol::PromptAssemblyRequest {
            session_id: request.session_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            run_id: run_id.to_string(),
            agent_spec: agent_spec.clone(),
            resources: request.prompt_resources.clone().unwrap_or_default(),
            tool_catalog,
        };
        let prompt = if let Some(port) = &scope.ports().prompt {
            port.assemble_prompt(assembly).await?
        } else {
            prompt::fallback_run_prompt(&assembly)
        };
        Ok(PreparedRunContext {
            prompt,
            tool_catalog: frozen_catalog,
            routes,
        })
    }

    pub(crate) async fn steer_execution(
        &self,
        request: SteerExecutionRequest,
    ) -> Result<ExecutionInputReceipt, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        let handle = scope
            .get_execution(&request.execution_id)
            .await
            .ok_or(AgentApiError::ExecutionNotFound)?;
        let (reply_tx, reply_rx) = piko_comms::reply::<ExecutionCommandReply, _>();
        handle
            .command_tx
            .try_send(ExecutionCommand::Steer {
                request: request.clone(),
                reply: reply_tx,
            })
            .map_err(|_| AgentApiError::Overload)?;
        reply_rx
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?
    }

    pub(crate) async fn request_cancel(
        &self,
        request: CancelExecutionRequest,
    ) -> Result<CancelReceipt, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        let handle = scope
            .get_execution(&request.execution_id)
            .await
            .ok_or(AgentApiError::ExecutionNotFound)?;
        handle.cancel.cancel();
        let (reply_tx, reply_rx) = piko_comms::reply::<ExecutionCommandReply, _>();
        let _ = handle.command_tx.try_send(ExecutionCommand::Cancel {
            request_id: request.request_id.clone(),
            reason: request.reason.clone(),
            reply: reply_tx,
        });
        match reply_rx.await {
            Ok(Ok(receipt)) => Ok(receipt),
            Ok(Err(err)) => Err(err),
            Err(_) => Ok(CancelReceipt {
                request_id: request.request_id,
                session_id: request.session_id,
                execution_id: request.execution_id,
                accepted: true,
            }),
        }
    }
}
