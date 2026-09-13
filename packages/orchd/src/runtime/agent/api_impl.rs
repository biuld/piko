use super::*;

#[path = "api_queries.rs"]
mod queries;

#[async_trait]
impl AgentRuntimeApi for AgentRuntime {
    async fn attach_agent_session(
        &self,
        config: SessionAgentConfig,
    ) -> Result<SessionAgentHandle, AgentApiError> {
        if config.root.session_id != config.session_id
            || config.root.parent_agent_instance_id.is_some()
        {
            return Err(AgentApiError::AgentParentMismatch);
        }
        let session_id = config.session_id.clone();
        let root = config.root.clone();
        let mut recovered_agents = config.recovered_agents;
        // Validate the full recovered tree before removing the root recovery
        // entry, and before any durable write or actor spawn.
        Self::validate_recovered_tree(&root, &recovered_agents)?;
        let root_recovery = recovered_agents
            .iter()
            .position(|state| state.identity.agent_instance_id == root.agent_instance_id)
            .map(|index| recovered_agents.remove(index));
        let root_spec = if let Some(recovery) = root_recovery.as_ref() {
            recovery.spec.clone()
        } else {
            self.execution
                .services()
                .agent_spec(&root.agent_spec_id)
                .await
                .ok_or(AgentApiError::AgentSpecNotFound)?
        };
        let scope = Arc::new(SessionAgentScope::new(
            session_id.clone(),
            root.agent_instance_id.clone(),
            Arc::clone(&config.ports.agents),
            self.agent_limits,
        ));

        // Reserve the session before the durable Create commit: a duplicate
        // or concurrent attach must fail closed without external side effects.
        {
            let mut sessions = self.sessions.write().await;
            if sessions.contains_key(&session_id) {
                return Err(AgentApiError::SessionAlreadyAttached);
            }
            sessions.insert(
                session_id.clone(),
                SessionSlot::Attaching(Arc::clone(&scope)),
            );
        }

        if let Err(error) = self
            .execution
            .attach_session(session_id.clone(), config.ports.executions)
            .await
        {
            Self::remove_session_if_scope(self, &session_id, &scope).await;
            return Err(error);
        }

        if let Err(error) = self
            .commit_agent_root(&scope, &root, root_spec, root_recovery)
            .await
        {
            Self::cleanup_session(self, &session_id, &scope).await;
            return Err(error);
        }
        for recovered in recovered_agents {
            let identity = recovered.identity.clone();
            if let Err(error) = self
                .spawn_agent_actor(&scope, identity, None, Some(recovered))
                .await
            {
                Self::cleanup_session(self, &session_id, &scope).await;
                return Err(error);
            }
        }
        {
            let mut sessions = self.sessions.write().await;
            match sessions.get(&session_id) {
                Some(SessionSlot::Attaching(current)) if Arc::ptr_eq(current, &scope) => {
                    sessions.insert(session_id.clone(), SessionSlot::Ready(Arc::clone(&scope)));
                }
                _ => {
                    drop(sessions);
                    Self::cleanup_session(self, &session_id, &scope).await;
                    return Err(AgentApiError::RuntimeUnavailable);
                }
            }
        }
        scope.activate();
        Ok(SessionAgentHandle {
            session_id,
            root_agent_instance_id: root.agent_instance_id,
        })
    }

    async fn detach_agent_session(&self, session_id: String) -> Result<(), AgentApiError> {
        let scope = {
            let mut sessions = self.sessions.write().await;
            let scope = match sessions.get(&session_id) {
                Some(SessionSlot::Ready(scope)) => Arc::clone(scope),
                Some(SessionSlot::Attaching(_)) => {
                    return Err(AgentApiError::RuntimeUnavailable);
                }
                None => return Err(AgentApiError::SessionNotAttached),
            };
            sessions.remove(&session_id);
            scope
        };
        scope.shutdown().await;
        self.execution.detach_session(session_id).await
    }

    async fn create_agent(
        &self,
        request: CreateAgentRequest,
    ) -> Result<CreateAgentReceipt, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        // The scope is bound to one session; a recovered actor committing a
        // foreign session_id through it must never resolve here.
        if scope.session_id() != request.session_id {
            return Err(AgentApiError::AgentParentMismatch);
        }
        let _create_guard = scope.lock_create().await;
        if let Some(receipt) = scope.create_receipt(&request).await? {
            return Ok(receipt);
        }
        scope
            .authorize_child_creation(&request.parent_agent_instance_id)
            .await?;
        let agent_instance_id = request
            .requested_agent_instance_id
            .clone()
            .unwrap_or_else(|| format!("agent_{}", Uuid::new_v4()));
        if let Some(existing) = scope.agent(&agent_instance_id).await {
            let identity = existing.snapshot_rx.borrow().identity.clone();
            if identity.agent_spec_id == request.agent_spec_id
                && identity.parent_agent_instance_id.as_deref()
                    == Some(request.parent_agent_instance_id.as_str())
            {
                let receipt = CreateAgentReceipt {
                    request_id: request.request_id.clone(),
                    identity,
                };
                scope.record_create(request, receipt.clone()).await;
                return Ok(receipt);
            }
            return Err(AgentApiError::AgentAlreadyExists);
        }
        scope
            .validate_new_child(&request.parent_agent_instance_id)
            .await?;
        let identity = AgentInstanceIdentity {
            session_id: request.session_id.clone(),
            agent_instance_id,
            agent_spec_id: request.agent_spec_id.clone(),
            parent_agent_instance_id: Some(request.parent_agent_instance_id.clone()),
        };
        let spec = self
            .execution
            .services()
            .agent_spec(&identity.agent_spec_id)
            .await
            .ok_or(AgentApiError::AgentSpecNotFound)?;
        scope
            .commit()
            .commit_agent_command(
                &request.session_id,
                AgentDurableCommand::Create {
                    identity: identity.clone(),
                    spec: spec.clone(),
                    origin_root_input_id: request.origin_root_input_id.clone(),
                    origin_tool_call_id: request.origin_tool_call_id.clone(),
                },
            )
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        self.spawn_agent_actor(&scope, identity.clone(), Some(spec), None)
            .await?;
        let receipt = CreateAgentReceipt {
            request_id: request.request_id.clone(),
            identity,
        };
        scope.record_create(request, receipt.clone()).await;
        Ok(receipt)
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        self.submit_runtime_agent_input(input, piko_orchd_api::AgentInputRuntime::default())
            .await
    }

    async fn submit_runtime_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        let parent = tracing::Span::current();
        let scope = self.scope(&input.session_id).await?;
        scope
            .authorize_input(
                input.caller_agent_instance_id.as_deref(),
                &input.agent_instance_id,
            )
            .await?;
        let handle = scope
            .agent(&input.agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let input_id = input.input_id.clone();
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .try_send(AgentCommand::Input {
                input,
                runtime,
                reply,
                parent,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => AgentApiError::Overload,
                mpsc::error::TrySendError::Closed(_) => AgentApiError::RuntimeUnavailable,
            })?;
        let mut receipt = received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)??;
        receipt.input_id = input_id;
        Ok(receipt)
    }

    async fn submit_agent_input_detached(
        &self,
        input: piko_protocol::AgentInput,
        recipient_agent_instance_id: String,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        let parent = tracing::Span::current();
        let scope = self.scope(&input.session_id).await?;
        scope
            .authorize_input(
                input.caller_agent_instance_id.as_deref(),
                &input.agent_instance_id,
            )
            .await?;
        let source = scope
            .agent(&input.agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        scope
            .agent(&recipient_agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        source
            .command_tx
            .try_send(AgentCommand::InputDetached {
                input,
                recipient: self::mailbox::DetachedReportTarget {
                    agent_instance_id: recipient_agent_instance_id,
                },
                reply,
                parent,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => AgentApiError::Overload,
                mpsc::error::TrySendError::Closed(_) => AgentApiError::RuntimeUnavailable,
            })?;
        received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?
    }

    async fn interrupt_agent(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentInterruptReceipt, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let Some(root_input_id) = handle.run_cancellation.cancel_active() else {
            return Ok(AgentInterruptReceipt {
                session_id,
                agent_instance_id,
                accepted: false,
            });
        };
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .send(AgentCommand::CancelRun {
                request_id: format!("cancel-agent-{agent_instance_id}"),
                expected_root_input_id: Some(root_input_id),
                reply,
            })
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        let result = received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        let accepted = match result {
            Err(AgentApiError::InvalidState) => true,
            Ok(receipt) => receipt.accepted,
            Err(error) => return Err(error),
        };
        Ok(AgentInterruptReceipt {
            session_id,
            agent_instance_id,
            accepted,
        })
    }

    async fn interrupt_agent_if_active(
        &self,
        session_id: String,
        agent_instance_id: String,
        root_input_id: String,
    ) -> Result<AgentInterruptReceipt, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let cancellation_requested = handle
            .run_cancellation
            .cancel_active_if_root(&root_input_id);
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .send(AgentCommand::CancelRun {
                request_id: format!("cancel-agent-input-{root_input_id}"),
                expected_root_input_id: Some(root_input_id),
                reply,
            })
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        let result = received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        let accepted = match result {
            Ok(receipt) => receipt.accepted,
            Err(AgentApiError::InvalidState) => cancellation_requested,
            Err(error) => return Err(error),
        };
        Ok(AgentInterruptReceipt {
            session_id,
            agent_instance_id,
            accepted,
        })
    }

    async fn cancel_agent_input(
        &self,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<piko_protocol::AgentInputCancelReceipt, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .send(AgentCommand::CancelInput { input_id, reply })
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?
    }

    async fn close_agent(
        &self,
        request: AgentLifecycleRequest,
    ) -> Result<AgentLifecycleReceipt, AgentApiError> {
        self.set_lifecycle(request, AgentInstanceLifecycle::Closed)
            .await
    }

    async fn reopen_agent(
        &self,
        request: AgentLifecycleRequest,
    ) -> Result<AgentLifecycleReceipt, AgentApiError> {
        self.set_lifecycle(request, AgentInstanceLifecycle::Open)
            .await
    }

    async fn agent_snapshot(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<Option<AgentSnapshot>, AgentApiError> {
        self.agent_snapshot_impl(session_id, agent_instance_id)
            .await
    }

    async fn wait_agent_input_started(
        &self,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<(), AgentApiError> {
        self.wait_agent_input_started_impl(session_id, agent_instance_id, input_id)
            .await
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
    ) -> Result<piko_protocol::AgentWorkReport, AgentApiError> {
        self.wait_agent_input_completion_impl(session_id, agent_instance_id, input_id)
            .await
    }

    async fn list_agents(&self, session_id: String) -> Result<Vec<AgentSnapshot>, AgentApiError> {
        self.list_agents_impl(session_id).await
    }

    async fn list_agent_specs(&self) -> Result<Vec<piko_protocol::AgentSpec>, AgentApiError> {
        self.list_agent_specs_impl().await
    }

    async fn agent_inbox(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentInboxSnapshot, AgentApiError> {
        self.agent_inbox_impl(session_id, agent_instance_id).await
    }

    async fn consume_agent_inbox_item(
        &self,
        request: piko_protocol::ConsumeAgentInboxRequest,
    ) -> Result<piko_protocol::ConsumeAgentInboxReceipt, AgentApiError> {
        self.consume_agent_inbox_item_impl(request).await
    }

    async fn wait_agent_mailbox(
        &self,
        request: MailboxWaitRequest,
    ) -> Result<MailboxWaitSummary, AgentApiError> {
        self.wait_agent_mailbox_impl(request).await
    }
}
