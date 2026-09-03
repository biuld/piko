use async_trait::async_trait;
use std::sync::Arc;

use piko_orchd_api::{AgentInputRuntime, AgentRuntimeApi, SessionSubscription};
use piko_protocol::AgentInstanceLifecycle;

use crate::api::{ProtocolError, UserInteractionResponse};
use crate::ports::{AgentRunRunner, TrajectoryRegistryPort};

use super::OrchAgentRunRunner;

#[async_trait]
impl AgentRunRunner for OrchAgentRunRunner {
    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        runtime: AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, ProtocolError> {
        self.submit_runtime(input, runtime).await
    }

    async fn cancel_agent_input(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_protocol::AgentInputCancelReceipt, ProtocolError> {
        self.agent_runtime
            .cancel_agent_input(
                session_id.to_string(),
                agent_instance_id.to_string(),
                input_id.to_string(),
            )
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))
    }

    fn trajectory_registry(&self) -> Arc<dyn TrajectoryRegistryPort> {
        Arc::new(self.trajectory_recorders.clone())
    }

    async fn list_processes(&self) -> Vec<piko_protocol::command::ProcessInfo> {
        self.agent_runtime.list_processes()
    }

    async fn terminate_process(
        &self,
        process_id: &str,
    ) -> Option<piko_protocol::command::ProcessExit> {
        self.agent_runtime.stop_process(process_id).await
    }

    async fn mcp_statuses(&self) -> Vec<piko_protocol::command::McpServerInfo> {
        self.mcp_server_statuses.clone()
    }

    async fn seed_todo_lists(&self, lists: Vec<piko_protocol::TodoList>) {
        self.agent_runtime.seed_todo_lists(lists).await;
    }

    fn set_context_window_callback(&self, callback: piko_orchd::tools::NewContextWindowCallback) {
        self.set_context_window_callback(callback);
    }

    fn set_guardian_review_callback(
        &self,
        callback: crate::domain::guardian::GuardianReviewCallback,
    ) {
        self.set_guardian_review_callback(callback);
    }

    async fn ensure_session_runtime(
        &self,
        session_id: &str,
        cwd: &str,
        session_dir: &std::path::Path,
        resume_agent: Option<&crate::ports::ResumeAgent>,
    ) -> Result<(), ProtocolError> {
        self.ensure_session_runtime(session_id, cwd, session_dir, resume_agent)
            .await
    }

    async fn invalidate_session_runtime(&self, session_id: &str) -> Result<(), ProtocolError> {
        match self
            .agent_runtime
            .detach_agent_session(session_id.to_string())
            .await
        {
            Ok(()) | Err(piko_orchd_api::AgentApiError::SessionNotAttached) => Ok(()),
            Err(error) => Err(ProtocolError::InvalidCommand(error.to_string())),
        }
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<SessionSubscription, ProtocolError> {
        self.wait_started(session_id, agent_instance_id, input_id, disposition)
            .await
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<crate::ports::AgentRunCompletion, ProtocolError> {
        self.wait_completion(session_id, agent_instance_id, input_id)
            .await
    }

    async fn finish_agent_run(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        operation_id: &str,
    ) {
        self.finish_agent_input(session_id, operation_id);
        self.observation_router.unregister(session_id, operation_id);
        let _ = agent_instance_id;
    }

    async fn recover_observation(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        operation_id: &str,
    ) -> Result<
        (
            piko_protocol::agent_runtime::SessionRuntimeSnapshot,
            SessionSubscription,
        ),
        ProtocolError,
    > {
        // Resubscribe observation without cancelling the Agent run.
        let hub = self.hub_for_input(session_id, operation_id);
        let Some(hub) = hub else {
            return Err(ProtocolError::ObservationFailed(format!(
                "no live observation hub for {session_id}/{agent_instance_id}/{operation_id}"
            )));
        };
        let cursor = hub.cursor();
        let hub_sub = hub
            .subscribe(&cursor)
            .await
            .map_err(|reason| ProtocolError::ObservationFailed(reason.to_string()))?;
        let snapshot = piko_protocol::agent_runtime::SessionRuntimeSnapshot {
            session_id: session_id.to_string(),
            root_agent_instance_id: Some(format!("agent_{session_id}_root")),
            active_agent_instance_id: Some(agent_instance_id.to_string()),
            cursor: cursor.clone(),
        };
        Ok((
            snapshot,
            SessionSubscription {
                session_id: session_id.to_string(),
                cursor: cursor.clone(),
                output: piko_orchd::events::merged_output_stream(hub_sub, cursor),
            },
        ))
    }

    async fn interrupt_agent(&self, session_id: &str, agent_instance_id: &str) -> bool {
        let root_input_id = self
            .agent_runtime
            .list_agents(session_id.to_string())
            .await
            .ok()
            .and_then(|agents| {
                agents
                    .into_iter()
                    .find(|agent| agent.identity.agent_instance_id == agent_instance_id)
                    .and_then(|agent| agent.active_root_input_id)
            });
        let Some(root_input_id) = root_input_id else {
            return false;
        };
        if let Err(error) = self
            .commit_agent_work_fact(
                session_id,
                piko_protocol::AgentDurableCommand::InterruptRequested {
                    agent_instance_id: agent_instance_id.to_string(),
                    root_input_id,
                    requested_at: crate::util::now_ms(),
                },
            )
            .await
        {
            tracing::error!(%error, %session_id, %agent_instance_id, "failed to persist interrupt intent");
            return false;
        }
        self.agent_runtime
            .interrupt_agent(session_id.to_string(), agent_instance_id.to_string())
            .await
            .map(|receipt| receipt.accepted)
            .unwrap_or(false)
    }

    async fn cancel_queued_agent_run(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> bool {
        let accepted = self
            .agent_runtime
            .cancel_agent_input(
                session_id.to_string(),
                agent_instance_id.to_string(),
                input_id.to_string(),
            )
            .await
            .map(|receipt| receipt.accepted)
            .unwrap_or(false);
        if accepted {
            self.finish_agent_input(session_id, input_id);
        }
        accepted
    }

    async fn has_active_session_run(&self, session_id: &str) -> bool {
        self.active_agent_inputs
            .lock()
            .unwrap()
            .keys()
            .any(|(active_session_id, _)| active_session_id == session_id)
    }

    async fn list_agent_instances(&self, session_id: &str) -> Option<Vec<crate::api::AgentInfo>> {
        let snapshots = self
            .agent_runtime
            .list_agents(session_id.to_string())
            .await
            .ok()?;
        Some(
            snapshots
                .into_iter()
                .map(|snapshot| {
                    let status = match (&snapshot.lifecycle, &snapshot.activity) {
                        (AgentInstanceLifecycle::Closed, _) => crate::api::AgentStatus::Closed,
                        (AgentInstanceLifecycle::Terminated, _) => crate::api::AgentStatus::Stopped,
                        (AgentInstanceLifecycle::Unavailable, _) => crate::api::AgentStatus::Failed,
                        (_, piko_protocol::AgentActivity::Running)
                        | (_, piko_protocol::AgentActivity::WaitingForApproval)
                        | (_, piko_protocol::AgentActivity::Cancelling) => {
                            crate::api::AgentStatus::Running
                        }
                        _ => crate::api::AgentStatus::Idle,
                    };
                    crate::api::AgentInfo {
                        session_id: session_id.to_string(),
                        agent_instance_id: snapshot.identity.agent_instance_id.clone(),
                        agent_id: snapshot.identity.agent_spec_id.clone(),
                        parent_agent_instance_id: snapshot
                            .identity
                            .parent_agent_instance_id
                            .clone(),
                        lifecycle: snapshot.lifecycle,
                        activity: snapshot.activity,
                        unread_report_count: snapshot.unread_report_count,
                        name: snapshot.identity.agent_spec_id,
                        role: "assistant".into(),
                        status,
                    }
                })
                .collect(),
        )
    }

    async fn respond_approval(
        &self,
        approval_id: &str,
        decision: crate::api::ApprovalDecision,
    ) -> Result<bool, ProtocolError> {
        let pending = self
            .pending_approvals
            .lock()
            .unwrap()
            .get(approval_id)
            .map(|entry| (entry.session_id.clone(), entry.snapshot.clone()));
        if let Some((Some(session_id), snapshot)) = pending {
            self.commit_agent_work_fact(
                &session_id,
                piko_protocol::AgentDurableCommand::PendingActionResolved {
                    agent_instance_id: snapshot.agent_instance_id,
                    root_input_id: snapshot.root_input_id,
                    action_id: approval_id.to_string(),
                    resolved_at: crate::util::now_ms(),
                },
            )
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        }
        let entry = self.pending_approvals.lock().unwrap().remove(approval_id);
        if let Some(entry) = entry {
            if let Some(session_id) = &entry.session_id {
                let status = match &decision {
                    crate::api::ApprovalDecision::Decline => crate::api::ApprovalStatus::Rejected,
                    crate::api::ApprovalDecision::Expired => crate::api::ApprovalStatus::Expired,
                    _ => crate::api::ApprovalStatus::Approved,
                };
                self.observation_router
                    .publish(
                        session_id,
                        &entry.snapshot.agent_instance_id,
                        "unknown",
                        piko_protocol::agent_runtime::SessionEvent::ApprovalResolved {
                            approval_id: approval_id.to_string(),
                            status,
                        },
                    )
                    .await;
            }
            let _ = entry.tx.send(decision);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn respond_user_interaction(
        &self,
        interaction_id: &str,
        response: UserInteractionResponse,
    ) -> Result<bool, ProtocolError> {
        let pending = self
            .pending_interactions
            .lock()
            .unwrap()
            .get(interaction_id)
            .map(|entry| (entry.session_id.clone(), entry.snapshot.clone()));
        if let Some((Some(session_id), snapshot)) = pending {
            self.commit_agent_work_fact(
                &session_id,
                piko_protocol::AgentDurableCommand::PendingActionResolved {
                    agent_instance_id: snapshot.agent_instance_id,
                    root_input_id: snapshot.root_input_id,
                    action_id: interaction_id.to_string(),
                    resolved_at: crate::util::now_ms(),
                },
            )
            .await
            .map_err(|error| ProtocolError::InvalidCommand(error.to_string()))?;
        }
        let entry = self
            .pending_interactions
            .lock()
            .unwrap()
            .remove(interaction_id);
        if let Some(entry) = entry {
            if let Some(session_id) = &entry.session_id {
                let status = match &response {
                    UserInteractionResponse::Submit { .. } => {
                        crate::api::UserInteractionStatus::Submitted
                    }
                    UserInteractionResponse::Cancel { .. } => {
                        crate::api::UserInteractionStatus::Cancelled
                    }
                };
                self.observation_router
                    .publish(
                        session_id,
                        &entry.snapshot.agent_instance_id,
                        &entry.snapshot.agent_id,
                        piko_protocol::agent_runtime::SessionEvent::InteractionResolved {
                            interaction_id: interaction_id.to_string(),
                            status,
                        },
                    )
                    .await;
            }
            let _ = entry.tx.send(response);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn pending_prompts_for_session(
        &self,
        session_id: &str,
    ) -> (
        Vec<crate::api::ApprovalSnapshot>,
        Vec<crate::api::UserInteractionSnapshot>,
    ) {
        let approvals = self
            .pending_approvals
            .lock()
            .unwrap()
            .values()
            .filter(|entry| entry.session_id.as_deref() == Some(session_id))
            .map(|entry| entry.snapshot.clone())
            .collect();
        let interactions = self
            .pending_interactions
            .lock()
            .unwrap()
            .values()
            .filter(|entry| entry.session_id.as_deref() == Some(session_id))
            .map(|entry| entry.snapshot.clone())
            .collect();
        (approvals, interactions)
    }
}
