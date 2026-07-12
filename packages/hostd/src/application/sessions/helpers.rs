use crate::api::ServerMessage;
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

pub(super) fn session_opened_messages(
    command_id: &str,
    session_id: String,
    snapshot: crate::api::SessionSnapshot,
    agents: Vec<crate::api::AgentInfo>,
    interrupt_events: Vec<ServerMessage>,
) -> Vec<ServerMessage> {
    let cursor = piko_protocol::agent_runtime::SessionCursor {
        epoch: format!("hostd:{session_id}"),
        seq: snapshot.seq,
    };
    let mut messages = interrupt_events;
    messages.push(server_response_ok(
        command_id,
        crate::api::CommandResult::SessionOpened {
            session_id: session_id.clone(),
            snapshot: snapshot.clone(),
            timestamp: now_ms(),
        },
    ));
    messages.push(ServerMessage::SessionReconciled(
        piko_protocol::SessionReconciledEvent {
            session_id,
            reason: piko_protocol::ReconcileReason::InitialHydration,
            cursor,
            snapshot,
            agents,
        },
    ));
    messages
}
