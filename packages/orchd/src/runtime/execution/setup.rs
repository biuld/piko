use super::*;

impl AgentExecutionRuntime {
    pub fn new(model_executor: Arc<dyn InferenceGateway>) -> Self {
        Self::with_telemetry(
            model_executor,
            Arc::new(piko_orchd_api::telemetry::NoopRuntimeTelemetry),
        )
    }

    pub fn with_telemetry(
        model_executor: Arc<dyn InferenceGateway>,
        telemetry: Arc<dyn piko_orchd_api::telemetry::RuntimeTelemetry>,
    ) -> Self {
        Self {
            services: ExecutionServices::with_telemetry(model_executor, telemetry),
            processes: Arc::new(ProcessManager::new()),
            sessions: RwLock::new(HashMap::new()),
            accepting: AtomicBool::new(true),
        }
    }

    pub async fn register_agent(&self, spec: AgentSpec) {
        self.services.register_agent(spec).await;
    }

    pub async fn register_tool_provider(&self, provider: Box<dyn piko_orchd_api::ToolProvider>) {
        self.services.register_tool_provider(provider).await;
    }

    pub async fn install_tool_contribution(
        &self,
        contribution: crate::adapters::tools::ToolContribution,
    ) -> Result<(), String> {
        self.services.install_tool_contribution(contribution).await
    }

    /// Snapshot of the live `process` tool set (hostd `/ps` surface).
    pub(crate) fn list_processes(&self) -> Vec<piko_protocol::command::ProcessInfo> {
        self.processes
            .list_processes()
            .into_iter()
            .map(|info| piko_protocol::command::ProcessInfo {
                process_id: info.process_id,
                pid: info.pid,
                command: info.command,
                cwd: info.cwd.display().to_string(),
                exited: info.exited,
                exit_code: info.exit_code,
                signal: info.signal,
            })
            .collect()
    }

    /// Terminate one process (group SIGTERM → SIGKILL) and report its exit.
    pub(crate) async fn stop_process(
        &self,
        process_id: &str,
    ) -> Option<piko_protocol::command::ProcessExit> {
        use piko_protocol::command::ProcessExit;
        self.processes
            .stop(process_id, std::time::Duration::from_secs(2))
            .await
            .map(|status| ProcessExit {
                exit_code: status.code,
                signal: status.signal,
            })
    }

    pub async fn register_tool_set(&self, tool_set: piko_protocol::tools::ToolSet) {
        self.services.register_tool_set(tool_set).await;
    }

    pub async fn set_approval_gateway(&self, gateway: Box<dyn piko_orchd_api::ApprovalGateway>) {
        self.services
            .tool_registry()
            .set_approval_gateway(Some(gateway))
            .await;
    }

    /// Seed the runtime todo store from host durable lists (session hydrate).
    pub async fn seed_todo_lists(&self, lists: impl IntoIterator<Item = piko_protocol::TodoList>) {
        self.services.seed_todo_lists(lists).await;
    }

    pub fn services(&self) -> &ExecutionServices {
        &self.services
    }

    pub(crate) async fn wait_terminal_state(
        &self,
        session_id: &str,
        root_input_id: &str,
    ) -> Result<ExecutionTerminal, AgentApiError> {
        let scope = self.scope(session_id).await?;
        if let Some(terminal) = scope.take_completed(root_input_id).await {
            return Ok(terminal);
        }
        let handle = scope
            .get_execution(root_input_id)
            .await
            .ok_or(AgentApiError::ExecutionNotFound)?;
        let terminal = handle.terminal_rx.wait().await?;
        let _ = scope.take_completed(root_input_id).await;
        Ok(terminal)
    }

    pub(super) async fn scope(
        &self,
        session_id: &str,
    ) -> Result<Arc<SessionExecutionScope>, AgentApiError> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or(AgentApiError::SessionNotAttached)
    }

    /// Commit an execution message for a session that is already attached.
    /// Used by the agent actor to make the startup-cancel abort marker
    /// durable before the run terminal (F-01 / D-01).
    pub(crate) async fn commit_execution_message(
        &self,
        session_id: &str,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<(), AgentApiError> {
        let scope = self.scope(session_id).await?;
        scope
            .ports()
            .commit
            .commit_message(commit)
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        Ok(())
    }
}
