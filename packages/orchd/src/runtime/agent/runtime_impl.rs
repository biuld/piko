use super::*;

impl AgentRuntime {
    pub fn new(model_executor: Arc<dyn InferenceGateway>) -> Self {
        Self {
            execution: Arc::new(AgentExecutionRuntime::new(model_executor)),
            sessions: RwLock::new(HashMap::new()),
            accepting: AtomicBool::new(true),
            context_tools: Arc::new(crate::adapters::tools::ContextToolsProvider::new()),
        }
    }

    /// Live set of external processes spawned by the workspace `process`
    /// tool, for the hostd `/ps` surface (mirrors codex-rs
    /// `list_background_terminals`).
    pub fn list_processes(&self) -> Vec<piko_protocol::command::ProcessInfo> {
        self.execution.list_processes()
    }

    /// Terminate one external process and report its exit (hostd `/kill`
    /// surface; mirrors codex-rs `terminate_background_terminal`).
    pub async fn stop_process(
        &self,
        process_id: &str,
    ) -> Option<piko_protocol::command::ProcessExit> {
        self.execution.stop_process(process_id).await
    }

    pub(super) fn from_execution_runtime(
        execution: Arc<AgentExecutionRuntime>,
        context_tools: Arc<crate::adapters::tools::ContextToolsProvider>,
    ) -> Self {
        Self {
            execution,
            sessions: RwLock::new(HashMap::new()),
            accepting: AtomicBool::new(true),
            context_tools,
        }
    }

    pub async fn bootstrap(
        model_executor: Arc<dyn InferenceGateway>,
        config: piko_protocol::config::OrchdConfig,
    ) -> Arc<Self> {
        Self::bootstrap_with_telemetry(
            model_executor,
            config,
            Arc::new(piko_orchd_api::telemetry::NoopRuntimeTelemetry),
        )
        .await
    }

    /// Like [`bootstrap`], with a hostd-provided telemetry sink for metrics.
    pub async fn bootstrap_with_telemetry(
        model_executor: Arc<dyn InferenceGateway>,
        config: piko_protocol::config::OrchdConfig,
        telemetry: Arc<dyn piko_orchd_api::telemetry::RuntimeTelemetry>,
    ) -> Arc<Self> {
        let execution =
            AgentExecutionRuntime::bootstrap_with_telemetry(model_executor, config, telemetry)
                .await;
        let context_tools_provider = crate::adapters::tools::ContextToolsProvider::new();
        let runtime = Arc::new(Self::from_execution_runtime(
            Arc::clone(&execution),
            Arc::new(context_tools_provider.clone()),
        ));
        execution
            .register_tool_provider(Box::new(
                crate::adapters::tools::MultiAgentToolProvider::new(
                    runtime.clone() as Arc<dyn AgentRuntimeApi>
                ),
            ))
            .await;
        execution
            .register_tool_set(piko_protocol::tools::ToolSet {
                id: "multi_agent".into(),
                name: "Multi-Agent Tools".into(),
                description: Some(
                    "Multi-agent: list_agent_specs, spawn, message_agent (queue/steer), list_agents, wait"
                        .into(),
                ),
                metadata: None,
                policy: None,
                tools: vec![piko_protocol::tools::ToolSetToolRef::ProviderNamespace {
                    provider_id: "multi_agent".into(),
                    namespace: "".into(),
                    alias: None,
                    policy: None,
                }],
            })
            .await;
        execution
            .register_tool_provider(Box::new(context_tools_provider))
            .await;
        execution
            .register_tool_set(piko_protocol::tools::ToolSet {
                id: "context".into(),
                name: "Context Budget Tools".into(),
                description: Some("Model-visible context budget and fresh-window tools".into()),
                metadata: None,
                policy: None,
                tools: vec![piko_protocol::tools::ToolSetToolRef::ProviderNamespace {
                    provider_id: "context".into(),
                    namespace: "".into(),
                    alias: None,
                    policy: None,
                }],
            })
            .await;
        runtime
    }

    pub async fn register_agent(&self, spec: piko_protocol::AgentSpec) {
        self.execution.register_agent(spec).await;
    }

    pub async fn register_tool_provider(&self, provider: Box<dyn piko_orchd_api::ToolProvider>) {
        self.execution.register_tool_provider(provider).await;
    }

    /// The context-budget tool provider (F-05). Hosts wire the
    /// `new_context_window` callback here.
    pub fn context_tools(&self) -> Arc<crate::adapters::tools::ContextToolsProvider> {
        Arc::clone(&self.context_tools)
    }

    pub async fn register_tool_set(&self, tool_set: piko_protocol::tools::ToolSet) {
        self.execution.register_tool_set(tool_set).await;
    }

    pub async fn set_approval_gateway(&self, gateway: Box<dyn piko_orchd_api::ApprovalGateway>) {
        self.execution.set_approval_gateway(gateway).await;
    }

    pub(super) async fn scope(
        &self,
        session_id: &str,
    ) -> Result<Arc<SessionAgentScope>, AgentApiError> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or(AgentApiError::SessionNotAttached)
    }

    pub(super) async fn spawn_agent_actor(
        &self,
        scope: &Arc<SessionAgentScope>,
        identity: AgentInstanceIdentity,
        spec_override: Option<piko_protocol::AgentSpec>,
        recovery: Option<AgentRecoveryState>,
    ) -> Result<(), AgentApiError> {
        // Recovery restores the durable immutable AgentSpec snapshot. The live
        // registry is authoritative only for newly created AgentInstances.
        let spec = if let Some(spec) = spec_override {
            spec
        } else if let Some(state) = recovery.as_ref() {
            state.spec.clone()
        } else if let Some(spec) = self
            .execution
            .services()
            .agent_spec(&identity.agent_spec_id)
            .await
        {
            spec
        } else {
            return Err(AgentApiError::AgentSpecNotFound);
        };
        let generation = scope.next_generation();
        let (command_tx, command_rx) = piko_comms::mailbox::<AgentCommands, _>();
        let lifecycle = recovery
            .as_ref()
            .map(|state| state.lifecycle)
            .unwrap_or(AgentInstanceLifecycle::Open);
        let transcript = recovery
            .as_ref()
            .map(|state| state.transcript.clone())
            .unwrap_or_default();
        let head_message_id = recovery
            .as_ref()
            .and_then(|state| state.head_message_id.clone());
        let inbox = recovery
            .as_ref()
            .map(|state| state.inbox.clone())
            .unwrap_or_default();
        let execution_reports = recovery
            .as_ref()
            .map(|state| state.execution_reports.clone())
            .unwrap_or_default();
        let queued_inputs = recovery
            .as_ref()
            .map(|state| state.queued_inputs.clone())
            .unwrap_or_default();
        let pending_detached_deliveries = recovery
            .as_ref()
            .map(|state| state.pending_detached_deliveries.clone())
            .unwrap_or_default();
        let latest_report = recovery.and_then(|state| state.latest_report);
        let initial = AgentSnapshot {
            identity: identity.clone(),
            lifecycle,
            activity: AgentActivity::Idle,
            latest_report: latest_report.clone(),
            unread_report_count: inbox
                .iter()
                .filter(|item| item.consumed_at.is_none())
                .count() as u32,
            generation,
        };
        let (snapshot_tx, snapshot_rx) = piko_comms::latest::<AgentSnapshotContract, _>(initial);
        let run_cancellation = Arc::new(RunCancellation::new());
        let handle = AgentHandle {
            generation,
            command_tx: command_tx.clone(),
            snapshot_rx,
            run_cancellation: Arc::clone(&run_cancellation),
        };
        scope
            .insert_agent(identity.agent_instance_id.clone(), handle)
            .await?;

        let actor = AgentActor::new(
            identity.clone(),
            spec,
            lifecycle,
            transcript,
            head_message_id,
            inbox,
            latest_report,
            execution_reports,
            queued_inputs,
            pending_detached_deliveries,
            generation,
            Arc::clone(scope.commit()),
            Arc::clone(&self.execution),
            command_tx.clone(),
            command_rx,
            snapshot_tx,
            Arc::downgrade(scope),
            run_cancellation,
        );
        let scope = Arc::clone(scope);
        let commit = Arc::clone(scope.commit());
        tokio::spawn(async move {
            let result = std::panic::AssertUnwindSafe(actor.run())
                .catch_unwind()
                .await;
            if result.is_err() {
                let _ = commit
                    .commit_agent_command(
                        &identity.session_id,
                        AgentDurableCommand::SetLifecycle {
                            agent_instance_id: identity.agent_instance_id.clone(),
                            lifecycle: AgentInstanceLifecycle::Unavailable,
                        },
                    )
                    .await;
            }
            scope
                .remove_if_generation(&identity.agent_instance_id, generation)
                .await;
        });
        Ok(())
    }

    pub(super) async fn set_lifecycle(
        &self,
        request: AgentLifecycleRequest,
        lifecycle: AgentInstanceLifecycle,
    ) -> Result<AgentLifecycleReceipt, AgentApiError> {
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
        scope
            .commit()
            .commit_agent_command(
                &request.session_id,
                AgentDurableCommand::SetLifecycle {
                    agent_instance_id: request.agent_instance_id.clone(),
                    lifecycle,
                },
            )
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        let (reply, received) = piko_comms::reply::<AgentCommandReply, _>();
        handle
            .command_tx
            .try_send(AgentCommand::SetLifecycle {
                request_id: request.request_id,
                lifecycle,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => AgentApiError::Overload,
                mpsc::error::TrySendError::Closed(_) => AgentApiError::RuntimeUnavailable,
            })?;
        received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?
    }
}
