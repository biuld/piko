use async_trait::async_trait;

use orchd_api::{AgentRuntimeApi, SessionSubscription};
use piko_protocol::{AgentInstanceLifecycle, MessageContent};

use crate::api::{ProtocolError, UserInteractionResponse};
use crate::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};

use super::OrchAgentRunRunner;
use super::run::root_agent_spec;

#[async_trait]
impl AgentRunRunner for OrchAgentRunRunner {
    async fn run_agent(&self, input: AgentRunInput) -> Result<AgentRunHandle, ProtocolError> {
        self.agent_runtime
            .set_approval_gateway(Box::new(self.clone()))
            .await;

        self.ui_router.register(
            &input.session_id,
            &input.operation_id,
            &input.agent_instance_id,
            input.agent_instance_id == format!("agent_{}_root", input.session_id),
            input.ui_event_tx.clone(),
        );
        self.register_user_interaction_tools_on_execution(self)
            .await;
        self.agent_runtime
            .register_agent(root_agent_spec(&input.cwd))
            .await;
        tracing::info!(
            session_id = %input.session_id,
            operation_id = %input.operation_id,
            agent_instance_id = %input.agent_instance_id,
            "Agent run subscription starting"
        );
        let session_id = input.session_id.clone();
        let operation_id = input.operation_id.clone();
        let result = self.run_agent_subscription(input).await;
        if result.is_err() {
            self.ui_router.unregister(&session_id, &operation_id);
        }
        result
    }

    async fn finish_agent_run(
        &self,
        operation: &crate::ports::AgentOperationAddress,
        _barrier: &piko_protocol::agent_runtime::SessionCursor,
    ) {
        self.finish_agent_input(
            &operation.session_id,
            &operation.agent_instance_id,
            &operation.operation_id,
        );
        self.ui_router
            .unregister(&operation.session_id, &operation.operation_id);
    }

    async fn recover_observation(
        &self,
        operation: &crate::ports::AgentOperationAddress,
    ) -> Result<
        (
            piko_protocol::agent_runtime::SessionRuntimeSnapshot,
            SessionSubscription,
        ),
        ProtocolError,
    > {
        // Resubscribe observation without cancelling the Agent run.
        let hub = self
            .active_agent_runs
            .lock()
            .unwrap()
            .get(&(
                operation.session_id.clone(),
                operation.agent_instance_id.clone(),
            ))
            .and_then(|run| {
                (run.run_id == operation.operation_id).then(|| run.observation.clone())
            });
        let Some(hub) = hub else {
            return Err(ProtocolError::ObservationFailed(format!(
                "no live observation hub for {}/{}/{}",
                operation.session_id, operation.agent_instance_id, operation.operation_id
            )));
        };
        let cursor = hub.cursor();
        let hub_sub = hub
            .subscribe(&cursor)
            .await
            .map_err(|reason| ProtocolError::ObservationFailed(reason.to_string()))?;
        let snapshot = piko_protocol::agent_runtime::SessionRuntimeSnapshot {
            session_id: operation.session_id.clone(),
            root_agent_instance_id: Some(format!("agent_{}_root", operation.session_id)),
            active_agent_instance_id: Some(operation.agent_instance_id.clone()),
            cursor: cursor.clone(),
        };
        Ok((
            snapshot,
            SessionSubscription {
                session_id: operation.session_id.clone(),
                cursor: cursor.clone(),
                output: orchd::events::merged_output_stream(hub_sub, cursor),
            },
        ))
    }

    async fn steer_agent(&self, session_id: &str, agent_instance_id: &str, message: &str) -> bool {
        if !self
            .active_agent_runs
            .lock()
            .unwrap()
            .contains_key(&(session_id.to_string(), agent_instance_id.to_string()))
        {
            return false;
        }
        self.agent_runtime
            .steer_agent(piko_protocol::SteerAgentRequest {
                request_id: format!("req_steer_{}", uuid::Uuid::new_v4()),
                session_id: session_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                caller_agent_instance_id: None,
                message_id: format!("msg_steer_{}", uuid::Uuid::new_v4()),
                content: MessageContent::String(message.to_string()),
            })
            .await
            .is_ok()
    }

    async fn cancel_agent_run(&self, operation: &crate::ports::AgentOperationAddress) -> bool {
        let active = {
            let active = self.active_agent_runs.lock().unwrap();
            active
                .get(&(
                    operation.session_id.clone(),
                    operation.agent_instance_id.clone(),
                ))
                .is_some_and(|run| run.run_id == operation.operation_id)
        };
        if !active {
            return false;
        }
        self.agent_runtime
            .cancel_agent_run(
                operation.session_id.clone(),
                operation.agent_instance_id.clone(),
            )
            .await
            .map(|receipt| receipt.accepted)
            .unwrap_or(false)
    }

    async fn has_active_session_run(&self, session_id: &str) -> bool {
        self.active_agent_runs
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
        let mut pending = self.pending_approvals.lock().unwrap();
        if let Some(entry) = pending.remove(approval_id) {
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
        let mut pending = self.pending_interactions.lock().unwrap();
        if let Some(entry) = pending.remove(interaction_id) {
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
