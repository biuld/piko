use async_trait::async_trait;

use orchd_api::{AgentRuntimeApi, SessionSubscription};
use piko_protocol::{AgentInstanceLifecycle, MessageContent};

use crate::api::{ProtocolError, UserInteractionResponse};
use crate::ports::{TurnRunInput, TurnRunner};

use super::OrchTurnRunner;
use super::run::root_agent_spec;

#[async_trait]
impl TurnRunner for OrchTurnRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, ProtocolError> {
        let gateway_runner = self.with_ui_event_tx(input.ui_event_tx.clone());

        self.agent_runtime
            .set_approval_gateway(Box::new(gateway_runner.clone()))
            .await;

        let agent_spec = root_agent_spec(
            &input.cwd,
            input.system_prompt.clone(),
            input.active_tool_names.clone(),
        );

        self.register_user_interaction_tools_on_execution(&gateway_runner)
            .await;
        self.agent_runtime.register_agent(agent_spec.clone()).await;
        tracing::info!(
            session_id = %input.session_id,
            turn_id = %input.turn_id,
            work_id = %input.work_id,
            "turn subscription starting; dispatching execution runtime"
        );
        self.run_execution_turn_subscription(input, agent_spec)
            .await
    }

    async fn recover_session_subscription(
        &self,
        session_id: &str,
    ) -> Result<
        (
            piko_protocol::agent_runtime::SessionRuntimeSnapshot,
            SessionSubscription,
        ),
        ProtocolError,
    > {
        // Resubscribe the live hub without cancelling the Execution Actor.
        let hub = {
            let hubs = self.active_hubs.lock().unwrap();
            hubs.get(session_id).cloned()
        };
        let Some(hub) = hub else {
            return Err(ProtocolError::ObservationFailed(format!(
                "no live execution observation hub for session {session_id}"
            )));
        };
        let root_agent_instance_id = format!("agent_{session_id}_root");
        let cursor = hub.cursor();
        let hub_sub = hub
            .subscribe(&cursor)
            .await
            .map_err(|reason| ProtocolError::ObservationFailed(reason.to_string()))?;
        let snapshot = piko_protocol::agent_runtime::SessionRuntimeSnapshot {
            session_id: session_id.to_string(),
            root_agent_instance_id: Some(root_agent_instance_id.clone()),
            active_agent_instance_id: Some(root_agent_instance_id),
            tasks: Vec::new(),
            cursor: cursor.clone(),
        };
        Ok((
            snapshot,
            SessionSubscription {
                session_id: session_id.to_string(),
                cursor: cursor.clone(),
                output: orchd::testing::merged_output_stream(hub_sub, cursor, None),
            },
        ))
    }

    async fn steer_task(
        &self,
        session_id: &str,
        _task_id: &str,
        _source_task_id: &str,
        _source_agent_id: &str,
        message: &str,
    ) -> bool {
        if !self.active_turns.lock().unwrap().contains_key(session_id) {
            return false;
        }
        self.agent_runtime
            .steer_agent(piko_protocol::SteerAgentRequest {
                request_id: format!("req_steer_{}", uuid::Uuid::new_v4()),
                session_id: session_id.to_string(),
                agent_instance_id: format!("agent_{session_id}_root"),
                caller_agent_instance_id: None,
                message_id: format!("msg_steer_{}", uuid::Uuid::new_v4()),
                content: MessageContent::String(message.to_string()),
            })
            .await
            .is_ok()
    }

    async fn cancel_execution(&self, session_id: &str, turn_id: &str) -> bool {
        let active = {
            let active = self.active_turns.lock().unwrap();
            active
                .get(session_id)
                .is_some_and(|active_turn| active_turn == turn_id)
        };
        if !active {
            return false;
        }
        self.agent_runtime
            .cancel_agent_run(session_id.to_string(), format!("agent_{session_id}_root"))
            .await
            .map(|receipt| receipt.accepted)
            .unwrap_or(false)
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
        if let Some(tx) = pending.remove(approval_id) {
            let _ = tx.send(decision);
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
        if let Some(tx) = pending.remove(interaction_id) {
            let _ = tx.send(response);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn on_task_created(&self, task_id: &str, session_id: &str, cwd: &str) {
        self.register_task_context(task_id.to_string(), session_id.to_string(), cwd.to_string());
    }
}
