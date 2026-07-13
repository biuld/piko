mod actor;
mod mailbox;
mod scope;

pub use scope::SessionAgentScope;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures_util::FutureExt;
use orchd_api::{
    AgentApiError, AgentRecoveryState, AgentRuntimeApi, SessionAgentConfig, SessionAgentHandle,
};
use piko_protocol::{
    AgentActivity, AgentDurableCommand, AgentInboxSnapshot, AgentInputReceipt,
    AgentInstanceIdentity, AgentInstanceLifecycle, AgentLifecycleReceipt, AgentLifecycleRequest,
    AgentSnapshot, CancelReceipt, CreateAgentReceipt, CreateAgentRequest, SendAgentInputRequest,
    SteerAgentRequest,
};
use tokio::sync::{RwLock, mpsc, oneshot, watch};
use uuid::Uuid;

use self::actor::AgentActor;
use self::mailbox::{AgentCommand, AgentHandle};
use super::execution::AgentExecutionRuntime;
use crate::ports::model_gateway::LlmGateway;

/// Mandatory facade and Actor supervisor for multi-agent runtime operations.
pub struct AgentRuntime {
    execution: Arc<AgentExecutionRuntime>,
    sessions: RwLock<HashMap<String, Arc<SessionAgentScope>>>,
    accepting: AtomicBool,
}

impl AgentRuntime {
    pub fn new(model_executor: Arc<dyn LlmGateway>) -> Self {
        Self {
            execution: Arc::new(AgentExecutionRuntime::new(model_executor)),
            sessions: RwLock::new(HashMap::new()),
            accepting: AtomicBool::new(true),
        }
    }

    fn from_execution_runtime(execution: Arc<AgentExecutionRuntime>) -> Self {
        Self {
            execution,
            sessions: RwLock::new(HashMap::new()),
            accepting: AtomicBool::new(true),
        }
    }

    pub async fn bootstrap(
        model_executor: Arc<dyn LlmGateway>,
        mut config: piko_protocol::config::OrchdConfig,
    ) -> Arc<Self> {
        for spec in config.agents.values_mut() {
            if !spec.tool_set_ids.iter().any(|id| id == "multi_agent") {
                spec.tool_set_ids.push("multi_agent".into());
            }
        }
        let execution = AgentExecutionRuntime::bootstrap(model_executor, config).await;
        let runtime = Arc::new(Self::from_execution_runtime(Arc::clone(&execution)));
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
                description: Some("AgentInstance creation, reuse, and status".into()),
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
        runtime
    }

    pub async fn register_agent(&self, spec: piko_protocol::AgentSpec) {
        self.execution.register_agent(spec).await;
    }

    pub async fn register_tool_provider(&self, provider: Box<dyn orchd_api::ToolProvider>) {
        self.execution.register_tool_provider(provider).await;
    }

    pub async fn register_tool_set(&self, tool_set: piko_protocol::tools::ToolSet) {
        self.execution.register_tool_set(tool_set).await;
    }

    pub async fn set_approval_gateway(&self, gateway: Box<dyn orchd_api::ApprovalGateway>) {
        self.execution.set_approval_gateway(gateway).await;
    }

    async fn scope(&self, session_id: &str) -> Result<Arc<SessionAgentScope>, AgentApiError> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or(AgentApiError::SessionNotAttached)
    }

    async fn spawn_agent_actor(
        &self,
        scope: &Arc<SessionAgentScope>,
        identity: AgentInstanceIdentity,
        spec_override: Option<piko_protocol::AgentSpec>,
        recovery: Option<AgentRecoveryState>,
    ) -> Result<(), AgentApiError> {
        let spec = match spec_override.or_else(|| recovery.as_ref().map(|state| state.spec.clone()))
        {
            Some(spec) => spec,
            None => self
                .execution
                .services()
                .agent_spec(&identity.agent_spec_id)
                .await
                .ok_or(AgentApiError::AgentSpecNotFound)?,
        };
        let generation = scope.next_generation();
        let (command_tx, command_rx) = mpsc::channel(32);
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
        let (snapshot_tx, snapshot_rx) = watch::channel(initial);
        let handle = AgentHandle {
            generation,
            command_tx: command_tx.clone(),
            snapshot_rx,
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

    async fn set_lifecycle(
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
        let (reply, received) = oneshot::channel();
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
        let scope = Arc::new(SessionAgentScope::new(
            session_id.clone(),
            root.agent_instance_id.clone(),
            Arc::clone(&config.ports.agents),
        ));

        config
            .ports
            .agents
            .commit_agent_command(
                &session_id,
                AgentDurableCommand::Create {
                    identity: root.clone(),
                    spec: self
                        .execution
                        .services()
                        .agent_spec(&root.agent_spec_id)
                        .await
                        .ok_or(AgentApiError::AgentSpecNotFound)?,
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
        let mut recovered_agents = config.recovered_agents;
        let root_recovery = recovered_agents
            .iter()
            .position(|state| state.identity.agent_instance_id == root.agent_instance_id)
            .map(|index| recovered_agents.remove(index));
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
        if scope
            .agent(&request.parent_agent_instance_id)
            .await
            .is_none()
        {
            return Err(AgentApiError::AgentNotFound);
        }
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
        let (reply, received) = oneshot::channel();
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
    ) -> Result<piko_protocol::AgentExecutionReport, AgentApiError> {
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
        let (reply, received) = oneshot::channel();
        handle
            .command_tx
            .try_send(AgentCommand::Run { request, reply })
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
        let (reply, received) = oneshot::channel();
        source
            .command_tx
            .try_send(AgentCommand::InputDetached {
                request,
                recipient: self::mailbox::DetachedReportTarget {
                    agent_instance_id: recipient_agent_instance_id,
                },
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

    async fn steer_agent(
        &self,
        request: SteerAgentRequest,
    ) -> Result<AgentInputReceipt, AgentApiError> {
        self.send_agent_input(SendAgentInputRequest {
            request_id: request.request_id,
            session_id: request.session_id,
            agent_instance_id: request.agent_instance_id,
            caller_agent_instance_id: request.caller_agent_instance_id,
            requested_execution_id: None,
            source_turn_id: None,
            message_id: request.message_id,
            content: request.content,
            delivery: piko_protocol::AgentInputDelivery::SteerActive,
        })
        .await
    }

    async fn cancel_agent_run(
        &self,
        session_id: String,
        agent_instance_id: String,
    ) -> Result<CancelReceipt, AgentApiError> {
        let scope = self.scope(&session_id).await?;
        let handle = scope
            .agent(&agent_instance_id)
            .await
            .ok_or(AgentApiError::AgentNotFound)?;
        let (reply, received) = oneshot::channel();
        handle
            .command_tx
            .send(AgentCommand::CancelRun {
                request_id: format!("cancel-agent-{agent_instance_id}"),
                reply,
            })
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
        let (reply, received) = oneshot::channel();
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
        let (reply, received) = oneshot::channel();
        handle
            .command_tx
            .send(AgentCommand::ConsumeInbox { request, reply })
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?;
        received
            .await
            .map_err(|_| AgentApiError::RuntimeUnavailable)?
    }
}

fn agent_depth(parents: &HashMap<String, Option<String>>, agent_instance_id: &str) -> usize {
    let mut depth = 0;
    let mut current = parents.get(agent_instance_id).and_then(Clone::clone);
    while let Some(parent) = current {
        depth += 1;
        if depth > parents.len() {
            break;
        }
        current = parents.get(&parent).and_then(Clone::clone);
    }
    depth
}
