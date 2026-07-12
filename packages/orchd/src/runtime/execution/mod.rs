//! Short-lived Execution Actor for the single-agent path.
//!
//! Temporary bridge: StepDispatch still keys identity with `task_id`; we pass
//! `execution_id` there until event consumers are renamed.

mod actor;
mod bootstrap;
mod finalizer;
mod mailbox;
mod scope;
mod services;
mod state;

pub use actor::ExecutionActor;
pub use finalizer::finalize_execution;
pub use mailbox::{ExecutionCommand, ExecutionHandle};
pub use scope::{ExecutionExit, SessionExecutionScope};
pub use services::ExecutionServices;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use orchd_api::{
    AgentApiError, AgentExecutor, CancelExecutionRequest, CancelReceipt, ExecutionInputReceipt,
    ExecutionReceipt, ExecutionSnapshot, ExecutionStatus, SessionExecutionConfig,
    SessionExecutionHandle, SessionSubscription, StartExecutionRequest, SteerExecutionRequest,
};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::ports::model_gateway::LlmGateway;
use piko_protocol::agents::AgentSpec;

/// Public facade and Execution Actor supervisor for the single-agent path.
pub struct AgentExecutionRuntime {
    services: ExecutionServices,
    sessions: RwLock<HashMap<String, Arc<SessionExecutionScope>>>,
    accepting: AtomicBool,
}

impl AgentExecutionRuntime {
    pub fn new(model_executor: Arc<dyn LlmGateway>) -> Self {
        Self {
            services: ExecutionServices::new(model_executor),
            sessions: RwLock::new(HashMap::new()),
            accepting: AtomicBool::new(true),
        }
    }

    pub async fn register_agent(&self, spec: AgentSpec) {
        self.services.register_agent(spec).await;
    }

    pub async fn register_tool_provider(&self, provider: Box<dyn orchd_api::ToolProvider>) {
        self.services.register_tool_provider(provider).await;
    }

    pub async fn register_tool_set(&self, tool_set: piko_protocol::tools::ToolSet) {
        self.services.register_tool_set(tool_set).await;
    }

    pub async fn set_approval_gateway(&self, gateway: Box<dyn orchd_api::ApprovalGateway>) {
        self.services
            .tool_registry()
            .set_approval_gateway(Some(gateway))
            .await;
    }

    pub fn services(&self) -> &ExecutionServices {
        &self.services
    }

    /// Wait for a started Execution to reach its durable terminal outcome.
    pub async fn wait_terminal(
        &self,
        session_id: &str,
        execution_id: &str,
    ) -> Result<piko_protocol::execution::ExecutionOutcome, AgentApiError> {
        let scope = self.scope(session_id).await?;
        if let Some(outcome) = scope.take_completed(execution_id).await {
            return Ok(outcome);
        }
        let handle = scope
            .get_execution(execution_id)
            .await
            .ok_or(AgentApiError::ExecutionNotFound)?;
        let outcome = handle.terminal_rx.wait().await?;
        let _ = scope.take_completed(execution_id).await;
        Ok(outcome)
    }

    async fn scope(&self, session_id: &str) -> Result<Arc<SessionExecutionScope>, AgentApiError> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or(AgentApiError::SessionNotAttached)
    }
}

#[async_trait]
impl AgentExecutor for AgentExecutionRuntime {
    async fn attach_session(
        &self,
        config: SessionExecutionConfig,
    ) -> Result<SessionExecutionHandle, AgentApiError> {
        if !self.accepting.load(Ordering::SeqCst) {
            return Err(AgentApiError::RuntimeUnavailable);
        }
        let mut sessions = self.sessions.write().await;
        if sessions.contains_key(&config.session_id) {
            return Err(AgentApiError::SessionAlreadyAttached);
        }
        let session_id = config.session_id.clone();
        sessions.insert(
            session_id.clone(),
            Arc::new(SessionExecutionScope::new(config)),
        );
        Ok(SessionExecutionHandle { session_id })
    }

    async fn detach_session(&self, session_id: String) -> Result<(), AgentApiError> {
        let scope = {
            let mut sessions = self.sessions.write().await;
            sessions
                .remove(&session_id)
                .ok_or(AgentApiError::SessionNotAttached)?
        };
        scope.cancel_all().await;
        Ok(())
    }

    async fn start_execution(
        &self,
        request: StartExecutionRequest,
    ) -> Result<ExecutionReceipt, AgentApiError> {
        if !self.accepting.load(Ordering::SeqCst) {
            return Err(AgentApiError::RuntimeUnavailable);
        }
        let scope = self.scope(&request.session_id).await?;
        let generation = scope.next_generation();
        let cancel = CancellationToken::new();
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(32);
        let (terminal_tx, terminal_rx) = tokio::sync::oneshot::channel();
        let (snapshot_tx, snapshot_rx) = tokio::sync::watch::channel(ExecutionSnapshot {
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            execution_id: request.execution_id.clone(),
            agent_id: request.config.agent_id.clone(),
            status: ExecutionStatus::Accepted,
            model_step_index: 0,
            usage: Default::default(),
            error: None,
        });

        let identity = ExecutionIdentity {
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            execution_id: request.execution_id.clone(),
            agent_id: request.config.agent_id.clone(),
        };

        let handle = ExecutionHandle {
            identity: identity.clone(),
            generation,
            command_tx,
            cancel: cancel.clone(),
            snapshot_rx,
            terminal_rx: crate::runtime::execution::mailbox::ArcTerminalReceiver::new(terminal_rx),
        };

        scope.reserve_execution(handle.clone()).await?;

        let receipt = ExecutionReceipt {
            request_id: request.request_id.clone(),
            session_id: identity.session_id.clone(),
            turn_id: identity.turn_id.clone(),
            execution_id: identity.execution_id.clone(),
            status: ExecutionStatus::Accepted,
        };

        let actor = ExecutionActor::new(
            identity,
            request,
            command_rx,
            cancel,
            Arc::clone(&scope),
            self.services.clone(),
            snapshot_tx,
        );

        let scope_for_supervise = Arc::clone(&scope);
        tokio::spawn(async move {
            let _exit =
                supervise_execution(scope_for_supervise, actor, generation, terminal_tx).await;
        });

        Ok(receipt)
    }

    async fn steer_execution(
        &self,
        request: SteerExecutionRequest,
    ) -> Result<ExecutionInputReceipt, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        let handle = scope
            .get_execution(&request.execution_id)
            .await
            .ok_or(AgentApiError::ExecutionNotFound)?;
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
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

    async fn request_cancel(
        &self,
        request: CancelExecutionRequest,
    ) -> Result<CancelReceipt, AgentApiError> {
        let scope = self.scope(&request.session_id).await?;
        let handle = scope
            .get_execution(&request.execution_id)
            .await
            .ok_or(AgentApiError::ExecutionNotFound)?;
        handle.cancel.cancel();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
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

    async fn execution_snapshot(
        &self,
        session_id: String,
        execution_id: String,
    ) -> Result<Option<ExecutionSnapshot>, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        Ok(scope
            .get_execution(&execution_id)
            .await
            .map(|handle| handle.snapshot_rx.borrow().clone()))
    }

    async fn subscribe_session(
        &self,
        _request: piko_protocol::agent_runtime::SubscribeRequest,
    ) -> Result<SessionSubscription, AgentApiError> {
        Err(AgentApiError::RuntimeUnavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionIdentity {
    pub session_id: String,
    pub turn_id: String,
    pub execution_id: String,
    pub agent_id: String,
}

async fn supervise_execution(
    scope: Arc<SessionExecutionScope>,
    actor: ExecutionActor,
    generation: u64,
    terminal_tx: tokio::sync::oneshot::Sender<piko_protocol::execution::ExecutionOutcome>,
) -> ExecutionExit {
    let identity = actor.identity().clone();
    let outcome = match actor.run().await {
        Ok(outcome) => outcome,
        Err(error) => {
            if matches!(error, AgentApiError::Cancelled) {
                piko_protocol::execution::ExecutionOutcome::Cancelled {
                    reason: Some("cancelled".into()),
                }
            } else {
                piko_protocol::execution::ExecutionOutcome::failed(error.to_string())
            }
        }
    };
    let terminal = finalize_execution(&scope, &identity, outcome).await;
    scope
        .publish_terminal(&identity.execution_id, terminal.clone())
        .await;
    let _ = terminal_tx.send(terminal.clone());
    scope
        .remove_if_generation(&identity.execution_id, generation)
        .await;
    ExecutionExit { identity, terminal }
}
