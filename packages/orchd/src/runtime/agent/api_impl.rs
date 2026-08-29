use super::*;

#[async_trait]
impl AgentRuntimeApi for AgentRuntime {
    async fn attach_agent_session(
        &self,
        config: SessionAgentConfig,
    ) -> Result<SessionAgentHandle, AgentApiError> {
        if !self.accepting.load(Ordering::SeqCst) {
            return Err(AgentApiError::RuntimeUnavailable);
        }
        if config.root.session_id != config.session_id
            || config.root.parent_agent_instance_id.is_some()
        {
            return Err(AgentApiError::AgentParentMismatch);
        }
        let session_id = config.session_id.clone();
        let root = config.root.clone();
        let mut recovered_agents = config.recovered_agents;
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

        config
            .ports
            .agents
            .commit_agent_command(
                &session_id,
                AgentDurableCommand::Create {
                    identity: root.clone(),
                    spec: root_spec,
                },
            )
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;

        {
            let mut sessions = self.sessions.write().await;
            if sessions.contains_key(&session_id) {
                return Err(AgentApiError::SessionAlreadyAttached);
            }
            sessions.insert(session_id.clone(), Arc::clone(&scope));
        }

        if let Err(error) = self
            .execution
            .attach_session(session_id.clone(), config.ports.executions)
            .await
        {
            self.sessions.write().await.remove(&session_id);
            return Err(error);
        }
        if let Err(error) = self
            .spawn_agent_actor(&scope, root.clone(), None, root_recovery)
            .await
        {
            self.sessions.write().await.remove(&session_id);
            let _ = self.execution.detach_session(session_id.clone()).await;
            return Err(error);
        }
        for recovered in recovered_agents {
            let identity = recovered.identity.clone();
            if let Err(error) = self
                .spawn_agent_actor(&scope, identity, None, Some(recovered))
                .await
            {
                scope.shutdown().await;
                self.sessions.write().await.remove(&session_id);
                let _ = self.execution.detach_session(session_id.clone()).await;
                return Err(error);
            }
        }
        Ok(SessionAgentHandle {
            session_id,
            root_agent_instance_id: root.agent_instance_id,
        })
    }

    async fn detach_agent_session(&self, session_id: String) -> Result<(), AgentApiError> {
        let scope = self
            .sessions
            .write()
            .await
            .remove(&session_id)
            .ok_or(AgentApiError::SessionNotAttached)?;
        scope.shutdown().await;
        self.execution.detach_session(session_id).await
    }

    async fn create_agent(
        &self,
        request: CreateAgentRequest,
    ) -> Result<CreateAgentReceipt, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
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

    async fn send_agent_input(
        &self,
        request: SendAgentInputRequest,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        scope
            .authorize_input(
                request.caller_agent_instance_id.as_deref(),
                &request.agent_instance_id,
            )
            .await?;
        let handle = scope
            .agent(&request.agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .try_send(AgentCommand::Input { request, reply })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => AgentApiError::Overload,
                mpsc::error::TrySendError::Closed(_) => AgentApiError::RuntimeUnavailable,
            })?;
        received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?
    }

    async fn run_agent(
        &self,
        request: SendAgentInputRequest,
    ) -> Result<piko_orchd_api::AgentRunAcceptance, AgentApiError> {
        let parent = tracing::Span::current();
        let scope = self.scope(&request.session_id).await?;
        scope
            .authorize_input(
                request.caller_agent_instance_id.as_deref(),
                &request.agent_instance_id,
            )
            .await?;
        let handle = scope
            .agent(&request.agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .try_send(AgentCommand::Run {
                request,
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

    async fn send_agent_input_detached(
        &self,
        request: SendAgentInputRequest,
        recipient_agent_instance_id: String,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        let parent = tracing::Span::current();
        let scope = self.scope(&request.session_id).await?;
        scope
            .authorize_input(
                request.caller_agent_instance_id.as_deref(),
                &request.agent_instance_id,
            )
            .await?;
        let source = scope
            .agent(&request.agent_instance_id)
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
                request,
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

    async fn steer_agent(
        &self,
        request: SteerAgentRequest,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        self.send_agent_input(SendAgentInputRequest {
            request_id: request.request_id,
            session_id: request.session_id,
            agent_instance_id: request.agent_instance_id,
            caller_agent_instance_id: request.caller_agent_instance_id,
            source_turn_id: None,
            message_id: request.message_id,
            content: request.content,
            delivery: piko_protocol::AgentInputDelivery::SteerActive,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
    }

    async fn cancel_agent_run(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentCancelReceipt, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let cancellation_requested = handle.run_cancellation.cancel_active();
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .send(AgentCommand::CancelRun {
                request_id: format!("cancel-agent-{agent_instance_id}"),
                reply,
            })
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        let result = received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        match result {
            Err(AgentApiError::InvalidState) if cancellation_requested => Ok(AgentCancelReceipt {
                request_id: format!("cancel-agent-{agent_instance_id}"),
                session_id,
                agent_instance_id,
                accepted: true,
            }),
            result => result,
        }
    }

    async fn cancel_agent_input(
        &self,
        session_id: String,
        agent_instance_id: String,
        request_id: String,
    ) -> Result<AgentCancelReceipt, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .send(AgentCommand::CancelInput { request_id, reply })
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
        let scope = self.scope(&session_id).await?;
        Ok(scope
            .agent(&agent_instance_id)
            .await
            .map(|handle| handle.snapshot_rx.borrow().clone()))
    }

    async fn list_agents(&self, session_id: String) -> Result<Vec<AgentSnapshot>, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let mut snapshots = scope.snapshots().await;
        let parents = snapshots
            .iter()
            .map(|snapshot| {
                (
                    snapshot.identity.agent_instance_id.clone(),
                    snapshot.identity.parent_agent_instance_id.clone(),
                )
            })
            .collect::<HashMap<_, _>>();
        snapshots.sort_by(|left, right| {
            agent_depth(&parents, &left.identity.agent_instance_id)
                .cmp(&agent_depth(&parents, &right.identity.agent_instance_id))
                .then_with(|| {
                    left.identity
                        .agent_instance_id
                        .cmp(&right.identity.agent_instance_id)
                })
        });
        Ok(snapshots)
    }

    async fn list_agent_specs(&self) -> Result<Vec<piko_protocol::AgentSpec>, AgentApiError> {
        Ok(self.execution.services().list_agent_specs().await)
    }

    async fn agent_inbox(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<AgentInboxSnapshot, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .send(AgentCommand::Inbox { reply })
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)
    }

    async fn consume_agent_inbox_item(
        &self,
        request: piko_protocol::ConsumeAgentInboxRequest,
    ) -> Result<piko_protocol::ConsumeAgentInboxReceipt, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        let handle = scope
            .agent(&request.agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .send(AgentCommand::ConsumeInbox { request, reply })
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?
    }

    async fn wait_agent_mailbox(
        &self,
        request: MailboxWaitRequest,
    ) -> Result<MailboxWaitSummary, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        if let Some(caller) = &request.caller_agent_instance_id {
            scope
                .agent(caller)
                .await
                .ok_or(AgentApiError::AgentNotFound)?;
        }
        let mut receiver = scope.mailbox_events().subscribe();
        let timeout = tokio::time::Duration::from_millis(request.timeout_ms);
        let event = tokio::time::timeout(timeout, async {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if request
                            .agent_instance_id
                            .as_deref()
                            .is_some_and(|filter| event.agent_instance_id() != filter)
                        {
                            continue;
                        }
                        return Some(event);
                    }
                    // Lagged events are skipped: waiting continues on the next
                    // update rather than failing or replaying.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        })
        .await
        .unwrap_or(None);

        let timed_out = event.is_none();
        let agents = self.list_agents(request.session_id).await?;
        Ok(MailboxWaitSummary {
            timed_out,
            event,
            agents,
        })
    }
}
