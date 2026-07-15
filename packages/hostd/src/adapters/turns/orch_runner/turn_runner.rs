use async_trait::async_trait;

use orchd_api::{AgentRuntimeApi, SessionSubscription};
use piko_protocol::{AgentInstanceLifecycle, MessageContent};

use crate::api::{ProtocolError, UserInteractionResponse};
use crate::ports::{
    AgentInputRunHandle, AgentInputRunInput, TurnRunHandle, TurnRunInput, TurnRunner,
};

use super::OrchTurnRunner;
use super::run::root_agent_spec;

#[async_trait]
impl TurnRunner for OrchTurnRunner {
    async fn run_turn(&self, input: TurnRunInput) -> Result<TurnRunHandle, ProtocolError> {
        self.agent_runtime
            .set_approval_gateway(Box::new(self.clone()))
            .await;

        let agent_spec = root_agent_spec(&input.cwd);

        self.ui_router.register(
            &input.session_id,
            &input.turn_id,
            &format!("agent_{}_root", input.session_id),
            true,
            input.ui_event_tx.clone(),
        );
        self.register_user_interaction_tools_on_execution(self)
            .await;
        self.agent_runtime.register_agent(agent_spec.clone()).await;
        tracing::info!(
            session_id = %input.session_id,
            turn_id = %input.turn_id,
            "turn subscription starting; dispatching execution runtime"
        );
        let session_id = input.session_id.clone();
        let turn_id = input.turn_id.clone();
        let result = self
            .run_execution_turn_subscription(input, agent_spec)
            .await;
        if result.is_err() {
            self.ui_router.unregister(&session_id, &turn_id);
        }
        result
    }

    async fn run_agent_input(
        &self,
        input: AgentInputRunInput,
    ) -> Result<AgentInputRunHandle, ProtocolError> {
        self.agent_runtime
            .set_approval_gateway(Box::new(self.clone()))
            .await;
        self.ui_router.register(
            &input.session_id,
            &input.run_id,
            &input.agent_instance_id,
            false,
            input.ui_event_tx.clone(),
        );
        self.register_user_interaction_tools_on_execution(self)
            .await;
        self.agent_runtime
            .register_agent(root_agent_spec(&input.cwd))
            .await;
        let session_id = input.session_id.clone();
        let run_id = input.run_id.clone();
        let result = self.run_agent_input_subscription(input).await;
        if result.is_err() {
            self.ui_router.unregister(&session_id, &run_id);
        }
        result
    }

    async fn finish_agent_input(&self, session_id: &str, agent_instance_id: &str, run_id: &str) {
        self.finish_agent_input(session_id, agent_instance_id, run_id);
        self.ui_router.unregister(session_id, run_id);
    }

    async fn acknowledge_turn_run(
        &self,
        session_id: &str,
        turn_id: &str,
        _barrier: &piko_protocol::agent_runtime::SessionCursor,
    ) {
        let mut active = self.active_turns.lock().unwrap();
        if let Some(runtime) =
            super::remove_active_turn_if_matches(&mut active, session_id, turn_id)
        {
            drop(active);
            runtime
                .commit_router
                .unregister(&runtime.root_agent_instance_id, turn_id);
            runtime
                .realtime_router
                .unregister(&runtime.root_agent_instance_id, turn_id);
            self.release_session_context_if_idle(session_id);
            self.ui_router.unregister(session_id, turn_id);
        }
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
        let root_hub = {
            let active = self.active_turns.lock().unwrap();
            active.get(&operation.session_id).and_then(|turn| {
                (turn.turn_id == operation.operation_id
                    && turn.root_agent_instance_id == operation.agent_instance_id)
                    .then(|| turn.observation.clone())
            })
        };
        let hub = root_hub.or_else(|| {
            self.active_agent_inputs
                .lock()
                .unwrap()
                .get(&(
                    operation.session_id.clone(),
                    operation.agent_instance_id.clone(),
                ))
                .and_then(|run| {
                    (run.run_id == operation.operation_id).then(|| run.observation.clone())
                })
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

    async fn steer_active_agent(&self, session_id: &str, message: &str) -> bool {
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

    async fn cancel_turn_run(&self, session_id: &str, turn_id: &str) -> bool {
        let active = {
            let active = self.active_turns.lock().unwrap();
            active
                .get(session_id)
                .is_some_and(|active_turn| active_turn.turn_id == turn_id)
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

    async fn has_active_turn_run(&self, session_id: &str) -> bool {
        self.active_turns.lock().unwrap().contains_key(session_id)
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
