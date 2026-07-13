//! Short-lived Execution Actor for the single-agent path.

mod actor;
mod bootstrap;
mod mailbox;
mod scope;
mod services;
pub(crate) mod state;

pub use actor::ExecutionActor;
pub use mailbox::{ExecutionCommand, ExecutionHandle};
pub use scope::{ExecutionExit, SessionExecutionScope};
pub use services::ExecutionServices;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::FutureExt;
use orchd_api::{AgentApiError, CancelReceipt, SessionExecutionPorts};
use piko_protocol::execution::{
    CancelExecutionRequest, ExecutionInputReceipt, ExecutionReceipt, ExecutionSnapshot,
    ExecutionStatus, StartExecutionRequest, SteerExecutionRequest,
};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::ports::model_gateway::LlmGateway;
use crate::runtime::reliability::TerminalSelector;
use piko_protocol::agents::AgentSpec;

/// AgentRuntime-internal Execution Actor supervisor.
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
    ) -> Result<PreparedExecution, AgentApiError> {
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
            source_turn_id: request.source_turn_id.clone(),
            execution_id: request.execution_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            agent_id: request.config.agent_id.clone(),
            status: ExecutionStatus::Accepted,
            model_step_index: 0,
            usage: Default::default(),
            error: None,
        });

        let identity = ExecutionIdentity {
            session_id: request.session_id.clone(),
            source_turn_id: request.source_turn_id.clone(),
            execution_id: request.execution_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            agent_id: request.config.agent_id.clone(),
        };

        let input_commit = piko_protocol::execution::MessageCommit {
            session_id: request.session_id.clone(),
            source_turn_id: request.source_turn_id.clone(),
            execution_id: request.execution_id.clone(),
            agent_instance_id: request.agent_instance_id.clone(),
            message_id: request.input_message_id.clone(),
            parent_message_id: request.context.head_message_id.clone(),
            message: piko_protocol::Message::User {
                content: request.input.clone(),
                timestamp: Some(chrono::Utc::now().timestamp_millis()),
            },
            committed_at: chrono::Utc::now().timestamp_millis(),
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
            source_turn_id: identity.source_turn_id.clone(),
            execution_id: identity.execution_id.clone(),
            agent_instance_id: identity.agent_instance_id.clone(),
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

        Ok(PreparedExecution {
            scope,
            actor: Some(actor),
            generation,
            terminal_tx: Some(terminal_tx),
            receipt,
            input_commit,
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
}

pub(crate) struct PreparedExecution {
    scope: Arc<SessionExecutionScope>,
    actor: Option<ExecutionActor>,
    generation: u64,
    terminal_tx: Option<tokio::sync::oneshot::Sender<ExecutionTerminal>>,
    receipt: ExecutionReceipt,
    input_commit: piko_protocol::execution::MessageCommit,
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
        tokio::spawn(async move {
            let _exit = supervise_execution(scope, actor, generation, terminal_tx).await;
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
    terminal_tx: tokio::sync::oneshot::Sender<ExecutionTerminal>,
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

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use async_trait::async_trait;
    use futures_core::Stream;
    use llmd::gateway::{GatewayEvent, GatewayRequest};
    use piko_protocol::execution::{CommitAck, CommitError};

    use super::*;

    struct NoopGateway;

    #[async_trait]
    impl LlmGateway for NoopGateway {
        async fn chat_stream(
            &self,
            _req: GatewayRequest,
            _cancel: Option<CancellationToken>,
        ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
            Ok(Box::pin(tokio_stream::empty()))
        }

        async fn llm_call(
            &self,
            _model: piko_protocol::Model,
            _system_prompt: Option<String>,
            _messages: Vec<piko_protocol::Message>,
            _settings: piko_protocol::model::ModelRunSettings,
        ) -> Result<String, String> {
            Ok(String::new())
        }

        fn capabilities(&self) -> piko_protocol::model::ModelCapabilities {
            piko_protocol::model::ModelCapabilities::default()
        }
    }

    struct NoopCommit;

    #[async_trait]
    impl orchd_api::ExecutionCommitPort for NoopCommit {
        async fn commit_message(
            &self,
            commit: piko_protocol::execution::MessageCommit,
        ) -> Result<CommitAck, CommitError> {
            Ok(CommitAck {
                session_id: commit.session_id,
                execution_id: commit.execution_id,
                agent_instance_id: commit.agent_instance_id,
                message_id: Some(commit.message_id),
                revision: 1,
            })
        }
    }

    fn request() -> StartExecutionRequest {
        StartExecutionRequest {
            request_id: "request".into(),
            session_id: "session".into(),
            source_turn_id: None,
            execution_id: "execution".into(),
            agent_instance_id: "agent".into(),
            agent_spec: AgentSpec {
                id: "main".into(),
                name: "main".into(),
                role: "test".into(),
                description: None,
                system_prompt: String::new(),
                model: None,
                thinking_level: None,
                tool_set_ids: Vec::new(),
                active_tool_names: None,
            },
            input_message_id: "message".into(),
            input: piko_protocol::MessageContent::String("hello".into()),
            context: piko_protocol::ConversationContext::empty(),
            config: piko_protocol::ExecutionConfig {
                agent_id: "main".into(),
                ..Default::default()
            },
        }
    }

    fn request_with(execution_id: &str, message_id: &str) -> StartExecutionRequest {
        StartExecutionRequest {
            execution_id: execution_id.into(),
            input_message_id: message_id.into(),
            ..request()
        }
    }

    #[tokio::test]
    async fn dropping_prepared_execution_releases_its_reservation() {
        let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
        runtime
            .attach_session(
                "session".into(),
                SessionExecutionPorts::new(Arc::new(NoopCommit)),
            )
            .await
            .unwrap();
        let prepared = runtime.prepare_execution(request()).await.unwrap();
        let scope = runtime.scope("session").await.unwrap();
        assert!(scope.get_execution("execution").await.is_some());
        drop(prepared);
        for _ in 0..100 {
            if scope.get_execution("execution").await.is_none() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("dropping PreparedExecution leaked its reservation");
    }

    #[tokio::test]
    async fn aborting_task_that_owns_prepared_execution_releases_reservation() {
        let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
        runtime
            .attach_session(
                "session".into(),
                SessionExecutionPorts::new(Arc::new(NoopCommit)),
            )
            .await
            .unwrap();
        let prepared = runtime.prepare_execution(request()).await.unwrap();
        let scope = runtime.scope("session").await.unwrap();
        let owner = tokio::spawn(async move {
            let _prepared = prepared;
            std::future::pending::<()>().await;
        });
        owner.abort();
        let _ = owner.await;
        for _ in 0..100 {
            if scope.get_execution("execution").await.is_none() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("aborting PreparedExecution owner leaked its reservation");
    }

    #[tokio::test]
    async fn prepare_failure_leaves_no_second_reservation_and_can_retry() {
        let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
        runtime
            .attach_session(
                "session".into(),
                SessionExecutionPorts::new(Arc::new(NoopCommit)),
            )
            .await
            .unwrap();
        let first = runtime
            .prepare_execution(request_with("first", "message-first"))
            .await
            .unwrap();
        assert!(matches!(
            runtime
                .prepare_execution(request_with("second", "message-second"))
                .await,
            Err(AgentApiError::ExecutionAlreadyActive)
        ));
        let scope = runtime.scope("session").await.unwrap();
        assert!(scope.get_execution("second").await.is_none());
        first.rollback().await;
        let retry = runtime
            .prepare_execution(request_with("second", "message-second"))
            .await
            .unwrap();
        retry.rollback().await;
    }
}
