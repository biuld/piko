use crate::api::{AgentInfo, ProtocolError, ServerMessage, SessionSnapshot};
use crate::application::host_app::HostApp;
use crate::util::now_ms;

pub(super) fn server_response_ok(
    command_id: &str,
    result: crate::api::CommandResult,
) -> ServerMessage {
    ServerMessage::CommandResponse {
        command_id: command_id.to_string(),
        result: Ok(result),
    }
}

pub(super) fn session_reconciled_message(
    session_id: String,
    reason: piko_protocol::ReconcileReason,
    snapshot: crate::api::SessionSnapshot,
    agents: Vec<crate::api::AgentInfo>,
) -> ServerMessage {
    let cursor = piko_protocol::agent_runtime::SessionCursor {
        epoch: format!("hostd:{session_id}"),
        seq: snapshot.seq,
    };
    ServerMessage::SessionReconciled(piko_protocol::SessionReconciledEvent {
        session_id,
        reason,
        cursor,
        snapshot,
        agents,
    })
}

pub(super) fn session_opened_messages(
    command_id: &str,
    session_id: String,
    snapshot: crate::api::SessionSnapshot,
    agents: Vec<crate::api::AgentInfo>,
    interrupt_events: Vec<ServerMessage>,
) -> Vec<ServerMessage> {
    let mut messages = interrupt_events;
    messages.push(server_response_ok(
        command_id,
        crate::api::CommandResult::SessionOpened {
            session_id: session_id.clone(),
            timestamp: now_ms(),
        },
    ));
    messages.push(session_reconciled_message(
        session_id,
        piko_protocol::ReconcileReason::InitialHydration,
        snapshot,
        agents,
    ));
    messages
}

impl HostApp {
    /// Enrich a domain snapshot with in-process pending prompts / live agents.
    pub(super) async fn enrich_session_view(
        &self,
        session_id: &str,
        mut snapshot: SessionSnapshot,
        mut agents: Vec<AgentInfo>,
    ) -> (SessionSnapshot, Vec<AgentInfo>) {
        let runner = self.turn_runner.lock().await.clone();
        if let Some(live_agents) = runner.list_agent_instances(session_id).await {
            agents = live_agents;
        }
        let (approvals, interactions) = runner.pending_prompts_for_session(session_id).await;
        snapshot.pending_approvals = approvals;
        snapshot.pending_interactions = interactions;
        if let Some(turn) = snapshot.active_turn.as_mut()
            && (!snapshot.pending_approvals.is_empty() || !snapshot.pending_interactions.is_empty())
        {
            turn.status = crate::api::TurnStatus::WaitingForApproval;
        }
        (snapshot, agents)
    }

    pub(super) async fn enrich_reconcile_messages(
        &self,
        session_id: &str,
        mut messages: Vec<ServerMessage>,
    ) -> Vec<ServerMessage> {
        for message in &mut messages {
            if let ServerMessage::SessionReconciled(reconciled) = message {
                let (snapshot, agents) = self
                    .enrich_session_view(
                        session_id,
                        reconciled.snapshot.clone(),
                        reconciled.agents.clone(),
                    )
                    .await;
                reconciled.snapshot = snapshot;
                reconciled.agents = agents;
                reconciled.cursor.seq = reconciled.snapshot.seq;
            }
        }
        messages
    }

    pub(super) async fn session_view(
        &self,
        session_id: &str,
    ) -> Result<(SessionSnapshot, Vec<AgentInfo>), ProtocolError> {
        let (snapshot, agents) = {
            let state = self.state.lock().await;
            (
                state.snapshot(session_id)?,
                state.get_agent_list(session_id),
            )
        };
        Ok(self.enrich_session_view(session_id, snapshot, agents).await)
    }
}
