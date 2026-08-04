//! Short-lived Execution Actor for the single-agent path.

mod actor;
mod bootstrap;
mod budget;
mod mailbox;
mod prompt;
mod scope;
mod services;
pub(crate) mod state;
mod tool_batch;

pub use actor::ExecutionActor;
pub use mailbox::{ExecutionCommand, ExecutionHandle};
pub use scope::{ExecutionExit, SessionExecutionScope};
pub use services::ExecutionServices;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::FutureExt;
use piko_comms::contracts::{
    ExecutionCommandReply, ExecutionCommands, ExecutionTerminal as ExecutionTerminalContract,
};
use piko_orchd_api::{AgentApiError, CancelReceipt, SessionExecutionPorts};
use piko_protocol::execution::{
    CancelExecutionRequest, ExecutionInputReceipt, ExecutionReceipt, ExecutionStatus,
    StartExecutionRequest, SteerExecutionRequest,
};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry};
use crate::ports::model_gateway::LlmGateway;
use crate::ports::tool_provider::ToolDiscoveryContext;
use crate::runtime::reliability::TerminalSelector;
use piko_protocol::agents::AgentSpec;
use piko_sandbox::exec::process::ProcessManager;

pub(crate) struct PreparedRunContext {
    pub prompt: piko_protocol::SemanticRunPrompt,
    pub tool_catalog: piko_protocol::ResolvedToolCatalog,
    pub routes: HashMap<String, CatalogRoute>,
}

/// AgentRuntime-internal Execution Actor supervisor.
pub struct AgentExecutionRuntime {
    services: ExecutionServices,
    /// Long-lived `process` tool manager, shared with the workspace
    /// provider and exposed for the hostd `/ps` surface (F-08).
    processes: Arc<ProcessManager>,
    sessions: RwLock<HashMap<String, Arc<SessionExecutionScope>>>,
    accepting: AtomicBool,
}

impl AgentExecutionRuntime {
    pub fn new(model_executor: Arc<dyn LlmGateway>) -> Self {
        Self::with_telemetry(
            model_executor,
            Arc::new(piko_orchd_api::telemetry::NoopRuntimeTelemetry),
        )
    }

    pub fn with_telemetry(
        model_executor: Arc<dyn LlmGateway>,
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

    pub fn services(&self) -> &ExecutionServices {
        &self.services
    }

    pub(crate) async fn wait_terminal_state(
        &self,
        session_id: &str,
        execution_id: &str,
    ) -> Result<ExecutionTerminal, AgentApiError> {
        let scope = self.scope(session_id).await?;
        if let Some(terminal) = scope.take_completed(execution_id).await {
            return Ok(terminal);
        }
        let handle = scope
            .get_execution(execution_id)
            .await
            .ok_or(AgentApiError::ExecutionNotFound)?;
        let terminal = handle.terminal_rx.wait().await?;
        let _ = scope.take_completed(execution_id).await;
        Ok(terminal)
    }

    async fn scope(&self, session_id: &str) -> Result<Arc<SessionExecutionScope>, AgentApiError> {
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
            execution_id: request.execution_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            agent_id: request.config.agent_id.clone(),
            agent_role,
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

pub(crate) struct PreparedExecution {
    scope: Arc<SessionExecutionScope>,
    actor: Option<ExecutionActor>,
    generation: u64,
    terminal_tx: Option<piko_comms::ReplySender<ExecutionTerminalContract, ExecutionTerminal>>,
    receipt: ExecutionReceipt,
    world_state_commit: Option<piko_protocol::execution::MessageCommit>,
    completion_commits: Vec<piko_protocol::execution::MessageCommit>,
    mention_commits: Vec<piko_protocol::execution::MessageCommit>,
    input_commit: piko_protocol::execution::MessageCommit,
    trace_span: tracing::Span,
}

impl PreparedExecution {
    pub fn identity(&self) -> &ExecutionIdentity {
        self.actor
            .as_ref()
            .expect("prepared Execution must own its Actor")
            .identity()
    }

    pub async fn activate(mut self) -> ExecutionReceipt {
        let actor = self
            .actor
            .take()
            .expect("prepared Execution owns its Actor until activation");
        let terminal_tx = self
            .terminal_tx
            .take()
            .expect("prepared Execution owns its terminal channel until activation");
        let scope = Arc::clone(&self.scope);
        let generation = self.generation;
        let trace_span = self.trace_span.clone();
        tokio::spawn(async move {
            let _exit = supervise_execution(scope, actor, generation, terminal_tx)
                .instrument(trace_span)
                .await;
        });
        self.receipt.clone()
    }

    pub fn receipt(&self) -> ExecutionReceipt {
        self.receipt.clone()
    }

    pub fn committed_input(&self) -> (piko_protocol::Message, String) {
        (
            self.input_commit.message.clone(),
            self.input_commit.message_id.clone(),
        )
    }

    pub async fn commit_input(&self) -> Result<(), AgentApiError> {
        if let Some(commit) = &self.world_state_commit {
            self.scope
                .ports()
                .commit
                .commit_message(commit.clone())
                .await
                .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        }
        for commit in &self.completion_commits {
            self.scope
                .ports()
                .commit
                .commit_message(commit.clone())
                .await
                .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        }
        for commit in &self.mention_commits {
            self.scope
                .ports()
                .commit
                .commit_message(commit.clone())
                .await
                .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        }
        self.scope
            .ports()
            .commit
            .commit_message(self.input_commit.clone())
            .await
            .map_err(|error| AgentApiError::PersistenceFailed(error.to_string()))?;
        Ok(())
    }

    pub async fn rollback(mut self) {
        let execution_id = self.identity().execution_id.clone();
        self.actor.take();
        self.terminal_tx.take();
        self.scope
            .rollback_reservation(&execution_id, self.generation)
            .await;
    }
}

impl Drop for PreparedExecution {
    fn drop(&mut self) {
        let Some(actor) = self.actor.as_ref() else {
            return;
        };
        let execution_id = actor.identity().execution_id.clone();
        let generation = self.generation;
        let scope = Arc::clone(&self.scope);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                scope.rollback_reservation(&execution_id, generation).await;
            });
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionIdentity {
    pub session_id: String,
    /// Interaction Turn this Execution is bound to. `None` for child agent
    /// Executions spawned by multi-agent tools.
    pub source_turn_id: Option<String>,
    pub execution_id: String,
    pub agent_instance_id: String,
    pub agent_id: String,
    /// F-19: role of the executing agent from the registered `AgentSpec`.
    /// `None` when the spec is not registered (inherits session policy).
    pub agent_role: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionTerminal {
    pub outcome: piko_protocol::execution::ExecutionOutcome,
    pub transcript: Vec<piko_protocol::Message>,
    pub head_message_id: Option<String>,
}

async fn supervise_execution(
    scope: Arc<SessionExecutionScope>,
    actor: ExecutionActor,
    generation: u64,
    terminal_tx: piko_comms::ReplySender<ExecutionTerminalContract, ExecutionTerminal>,
) -> ExecutionExit {
    let identity = actor.identity().clone();
    let result = std::panic::AssertUnwindSafe(actor.run())
        .catch_unwind()
        .await;
    let (outcome, transcript, head_message_id) = match result {
        Ok(result) => (result.outcome, result.transcript, result.head_message_id),
        Err(_) => (
            piko_protocol::ExecutionOutcome::failed("ExecutionActor panicked"),
            Vec::new(),
            None,
        ),
    };
    match &outcome {
        piko_protocol::execution::ExecutionOutcome::Succeeded { .. } => {
            tracing::info!(
                target: "agent.run_completed",
                session_id = %identity.session_id,
                run_id = %identity.execution_id,
                agent_instance_id = %identity.agent_instance_id,
                "Agent run completed"
            );
        }
        piko_protocol::execution::ExecutionOutcome::Cancelled { reason } => {
            tracing::Span::current().record("otel.status_code", "ERROR");
            tracing::info!(
                target: "agent.run_cancelled",
                session_id = %identity.session_id,
                run_id = %identity.execution_id,
                agent_instance_id = %identity.agent_instance_id,
                reason = ?reason,
                "Agent run cancelled"
            );
        }
        piko_protocol::execution::ExecutionOutcome::Failed { error } => {
            tracing::Span::current().record("otel.status_code", "ERROR");
            tracing::error!(
                target: "agent.run_failed",
                session_id = %identity.session_id,
                run_id = %identity.execution_id,
                agent_instance_id = %identity.agent_instance_id,
                error = %truncate(error, 512),
                "Agent run failed"
            );
        }
    }
    let candidate = ExecutionTerminal {
        outcome: outcome.clone(),
        transcript,
        head_message_id,
    };
    let mut selector = TerminalSelector::new();
    let _ = selector.choose(candidate);
    let terminal = selector
        .into_selected()
        .expect("Execution supervisor must select one terminal candidate");
    scope
        .publish_terminal(&identity.execution_id, terminal.clone())
        .await;
    let _ = terminal_tx.send(terminal.clone());
    scope
        .remove_if_generation(&identity.execution_id, generation)
        .await;
    ExecutionExit {
        identity,
        terminal: outcome,
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        let mut result = text.chars().take(max).collect::<String>();
        result.push_str("...");
        result
    }
}

#[cfg(test)]
mod tests;
